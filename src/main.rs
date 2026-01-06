//! Lo-phi: Feature Reduction CLI Tool
//!
//! A command-line tool for reducing features in datasets using
//! missing value analysis and correlation-based reduction.

mod cli;
mod pipeline;
mod report;
mod utils;

use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use console::style;

use cli::{run_config_menu, run_target_mapping_selector, Cli, Commands, Config, ConfigResult, TargetMappingResult};
use pipeline::{
    analyze_features_iv, analyze_missing_values, analyze_target_column, find_correlated_pairs,
    get_column_names, get_features_above_threshold, get_low_gini_features, get_weights,
    load_dataset_with_progress, select_features_to_drop, BinningStrategy, TargetAnalysis, TargetMapping,
};
use report::{export_gini_analysis_enhanced, ExportParams, ReductionSummary};
use utils::{
    create_spinner, finish_with_success, print_banner, print_completion, print_config,
    print_count, print_info, print_step_header, print_step_time, print_success,
};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle subcommands
    if let Some(command) = &cli.command {
        return match command {
            Commands::Convert {
                input,
                output,
                infer_schema_length,
                fast,
            } => cli::convert::run_convert(input, output.as_deref(), *infer_schema_length, *fast),
        };
    }

    // Main reduce pipeline - require input
    let input = cli.input().ok_or_else(|| {
        anyhow::anyhow!("Input file is required. Use -i/--input to specify a file.")
    })?;
    
    // Derive output path from input if not provided
    let output_path = cli.output_path().unwrap();

    // Build initial target mapping from CLI args if provided
    let cli_target_mapping = match (&cli.event_value, &cli.non_event_value) {
        (Some(event), Some(non_event)) => Some(TargetMapping::new(event.clone(), non_event.clone())),
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!("Both --event-value and --non-event-value must be provided together")
        }
        (None, None) => None,
    };

    // Determine final config values - either from interactive menu or CLI defaults
    let (target, missing_threshold, gini_threshold, gini_bins, correlation_threshold, columns_to_drop, mut target_mapping, weight_column) = if cli.no_confirm {
        // Skip interactive menu when --no-confirm is set
        // Target is required in non-interactive mode
        let target = cli.target.clone().ok_or_else(|| {
            anyhow::anyhow!("Target column is required when using --no-confirm. Use -t/--target to specify.")
        })?;
        (target, cli.missing_threshold, cli.gini_threshold, cli.gini_bins, cli.correlation_threshold, cli.drop_columns.clone(), cli_target_mapping, cli.weight_column.clone())
    } else {
        // Load column names for interactive selection
        let columns = get_column_names(input)?;
        
        // Show interactive config menu
        let config = Config {
            input: input.clone(),
            target: cli.target.clone(),
            output: output_path.clone(),
            missing_threshold: cli.missing_threshold,
            gini_threshold: cli.gini_threshold,
            correlation_threshold: cli.correlation_threshold,
            columns_to_drop: cli.drop_columns.clone(),
            target_mapping: cli_target_mapping.clone(),
            weight_column: cli.weight_column.clone(),
        };

        match run_config_menu(config, columns)? {
            ConfigResult::Proceed(cfg) => {
                let target = cfg.target.ok_or_else(|| {
                    anyhow::anyhow!("Target column must be selected before proceeding")
                })?;
                // Use config's target_mapping if set (from CLI), otherwise None (will be determined after loading data)
                (target, cfg.missing_threshold, cfg.gini_threshold, cli.gini_bins, cfg.correlation_threshold, cfg.columns_to_drop, cfg.target_mapping, cfg.weight_column)
            }
            ConfigResult::Quit => {
                println!("Cancelled by user.");
                return Ok(());
            }
        }
    };

    // Print styled banner
    print_banner(env!("CARGO_PKG_VERSION"));

    // Print configuration card
    print_config(
        input,
        &target,
        &output_path,
        missing_threshold,
        gini_threshold,
        correlation_threshold,
    );

    // Step 1: Load dataset (with progress bar for CSV files)
    let step_start = Instant::now();
    println!();  // Blank line before progress bar
    let (mut df, rows, cols, memory_mb) = load_dataset_with_progress(input, cli.infer_schema_length)?;
    print_success("Dataset loaded");

    // Display statistics (instant since data is already collected)
    println!("\n    {} Dataset Statistics:", style("✧").cyan());
    println!("      Rows: {}", rows);
    println!("      Columns: {}", cols);
    println!("      Estimated memory: {:.2} MB", memory_mb);

    // Apply user-specified column drops
    let dropped_count = if !columns_to_drop.is_empty() {
        let column_names: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
        let valid_columns: Vec<String> = columns_to_drop
            .iter()
            .filter(|col| column_names.contains(col))
            .cloned()
            .collect();
        let count = valid_columns.len();
        if count > 0 {
            df = df.drop_many(&valid_columns);
            print_success(&format!("Dropped {} user-specified column(s)", count));
        }
        count
    } else {
        0
    };

    let initial_features = cols - dropped_count;
    let mut summary = ReductionSummary::new(initial_features);
    let load_elapsed = step_start.elapsed();
    summary.set_load_time(load_elapsed);
    print_step_time(load_elapsed);

    // Verify target column exists
    let column_names: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    if !column_names.contains(&target) {
        anyhow::bail!(
            "Target column '{}' not found in dataset. Available columns: {:?}",
            target,
            column_names
        );
    }

    // Extract sample weights (defaults to equal weights of 1.0 if no weight column specified)
    let weights = get_weights(&df, weight_column.as_deref())?;
    if weight_column.is_some() {
        print_success(&format!(
            "Using weight column: '{}'",
            weight_column.as_ref().unwrap()
        ));
    }

    // Analyze target column to determine if mapping is needed
    if target_mapping.is_none() {
        match analyze_target_column(&df, &target)? {
            TargetAnalysis::AlreadyBinary => {
                // No mapping needed - target is already 0/1
            }
            TargetAnalysis::NeedsMapping { unique_values } => {
                if cli.no_confirm {
                    // In non-interactive mode, we need CLI args for mapping
                    anyhow::bail!(
                        "Target column '{}' is not binary (0/1). Found {} unique values: {:?}\n\
                         Use --event-value and --non-event-value to specify which values map to 1 and 0.",
                        target,
                        unique_values.len(),
                        unique_values
                    );
                }
                
                // Show interactive selector for event/non-event values
                println!();
                println!("   {} Target column '{}' is not binary (0/1)", style("⚠").yellow(), target);
                println!("     Found {} unique values: {:?}", unique_values.len(), &unique_values[..unique_values.len().min(5)]);
                if unique_values.len() > 5 {
                    println!("     ... and {} more", unique_values.len() - 5);
                }
                println!();
                
                match run_target_mapping_selector(unique_values)? {
                    TargetMappingResult::Selected(mapping) => {
                        println!("   {} Target mapping configured: '{}' → 1 (event), '{}' → 0 (non-event)",
                            style("✓").green(),
                            mapping.event_value,
                            mapping.non_event_value
                        );
                        target_mapping = Some(mapping);
                    }
                    TargetMappingResult::Cancelled => {
                        println!("Cancelled by user.");
                        return Ok(());
                    }
                }
            }
        }
    } else {
        // Mapping was provided via CLI - validate it
        let mapping = target_mapping.as_ref().unwrap();
        println!("   {} Using target mapping: '{}' → 1, '{}' → 0",
            style("✓").green(),
            mapping.event_value,
            mapping.non_event_value
        );
    }

    // Step 2: Missing value analysis
    // NOTE: All analysis functions now take &DataFrame to avoid repeated collection
    print_step_header(1, "Missing Value Analysis");

    let step_start = Instant::now();
    let spinner = create_spinner("Analyzing missing values...");
    let missing_ratios = analyze_missing_values(&df, &weights)?;
    let features_to_drop_missing = get_features_above_threshold(
        &missing_ratios,
        missing_threshold,
        &target,
    );
    finish_with_success(&spinner, "Missing value analysis complete");

    if features_to_drop_missing.is_empty() {
        print_info("No features exceed the missing value threshold");
    } else {
        print_count(
            "feature(s) with high missing values",
            features_to_drop_missing.len(),
            Some(&format!("(>{:.1}%)", missing_threshold * 100.0)),
        );

        // Drop columns directly from DataFrame
        df = df.drop_many(&features_to_drop_missing);
        summary.add_missing_drops(features_to_drop_missing);
        print_success("Dropped features with high missing values");
    }
    let missing_elapsed = step_start.elapsed();
    summary.set_missing_time(missing_elapsed);
    print_step_time(missing_elapsed);

    // Step 2: Univariate Gini Analysis
    print_step_header(2, "Univariate Gini Analysis");

    // Parse binning strategy
    let binning_strategy: BinningStrategy = cli.binning_strategy.parse().map_err(|e: String| {
        anyhow::anyhow!(e)
    })?;

    let step_start = Instant::now();
    let gini_analyses = analyze_features_iv(
        &df,
        &target,
        gini_bins,
        target_mapping.as_ref(),
        binning_strategy,
        Some(cli.min_category_samples),
        &weights,
    )?;
    let features_to_drop_gini = get_low_gini_features(&gini_analyses, gini_threshold);

    // Export Gini analysis to JSON for later inspection
    let gini_output_path = cli.gini_analysis_path().unwrap();
    let export_params = ExportParams {
        input_file: input.to_str().unwrap_or("unknown"),
        target_column: &target,
        weight_column: weight_column.as_deref(),
        binning_strategy,
        num_bins: gini_bins,
        gini_threshold,
        min_category_samples: cli.min_category_samples,
    };
    export_gini_analysis_enhanced(&gini_analyses, &features_to_drop_gini, &gini_output_path, &export_params)?;
    print_success(&format!("Gini analysis saved to {}", gini_output_path.display()));

    if features_to_drop_gini.is_empty() {
        print_info("No features below Gini threshold");
    } else {
        print_count(
            "feature(s) with low Gini",
            features_to_drop_gini.len(),
            Some(&format!("(<{:.2})", gini_threshold)),
        );

        // Drop columns directly from DataFrame
        df = df.drop_many(&features_to_drop_gini);
        summary.add_gini_drops(features_to_drop_gini);
        print_success("Dropped low Gini features");
    }

    let gini_elapsed = step_start.elapsed();
    summary.set_gini_time(gini_elapsed);
    print_step_time(gini_elapsed);

    // Step 3: Correlation analysis
    print_step_header(3, "Correlation Analysis");

    let step_start = Instant::now();
    let correlated_pairs = find_correlated_pairs(&df, correlation_threshold, &weights)?;
    let features_to_drop_corr = select_features_to_drop(&correlated_pairs, &target);
    print_success("Correlation analysis complete");

    if correlated_pairs.is_empty() {
        print_info("No highly correlated feature pairs found");
    } else {
        print_count(
            "correlated pair(s)",
            correlated_pairs.len(),
            Some(&format!("(>{:.2})", correlation_threshold)),
        );
        println!(
            "      Dropping {} feature(s)",
            style(features_to_drop_corr.len()).yellow().bold()
        );

        // Drop columns directly from DataFrame
        df = df.drop_many(&features_to_drop_corr);
        summary.add_correlation_drops(features_to_drop_corr);
        print_success("Dropped highly correlated features");
    }
    let correlation_elapsed = step_start.elapsed();
    summary.set_correlation_time(correlation_elapsed);
    print_step_time(correlation_elapsed);

    // Step 4: Save output
    print_step_header(4, "Save Results");

    let step_start = Instant::now();
    let spinner = create_spinner("Writing output file...");
    save_dataset(&mut df, &output_path)?;
    finish_with_success(
        &spinner,
        &format!("Saved to {}", output_path.display()),
    );
    let save_elapsed = step_start.elapsed();
    summary.set_save_time(save_elapsed);
    print_step_time(save_elapsed);

    // Display summary
    summary.display();

    // Final completion message
    print_completion();

    Ok(())
}

/// Save dataset to file (CSV or Parquet based on extension)
fn save_dataset(df: &mut polars::prelude::DataFrame, path: &std::path::Path) -> Result<()> {
    use anyhow::Context;
    use polars::prelude::*;

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "csv" => {
            let mut file = std::fs::File::create(path)
                .with_context(|| format!("Failed to create output file: {}", path.display()))?;
            CsvWriter::new(&mut file)
                .finish(df)
                .with_context(|| format!("Failed to write CSV file: {}", path.display()))?;
        }
        "parquet" => {
            let file = std::fs::File::create(path)
                .with_context(|| format!("Failed to create output file: {}", path.display()))?;
            ParquetWriter::new(file)
                .finish(df)
                .with_context(|| format!("Failed to write Parquet file: {}", path.display()))?;
        }
        _ => anyhow::bail!(
            "Unsupported output format: {}. Supported formats: csv, parquet",
            extension
        ),
    }

    Ok(())
}
