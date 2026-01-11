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

use cli::{
    run_config_menu, run_target_mapping_selector, Cli, Commands, Config, ConfigResult,
    TargetMappingResult,
};
use pipeline::{
    analyze_features_iv, analyze_missing_values, analyze_target_column, find_correlated_pairs_auto,
    get_column_names, get_features_above_threshold, get_low_gini_features, get_weights,
    load_dataset_with_progress, select_features_to_drop, BinningStrategy, MonotonicityConstraint,
    SolverConfig, TargetAnalysis, TargetMapping,
};
use report::{export_gini_analysis_enhanced, ExportParams, ReductionSummary};
use utils::{
    create_spinner, finish_with_success, print_banner, print_completion, print_config, print_count,
    print_info, print_step_header, print_step_time, print_success,
};

/// Configuration parameters for the reduction pipeline
struct PipelineConfig {
    target: String,
    missing_threshold: f64,
    gini_threshold: f64,
    gini_bins: usize,
    correlation_threshold: f64,
    columns_to_drop: Vec<String>,
    target_mapping: Option<TargetMapping>,
    weight_column: Option<String>,
}

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

    // Setup configuration (interactive or CLI-based)
    let mut config = setup_configuration(&cli, input, &output_path)?;

    // Print styled banner
    print_banner(env!("CARGO_PKG_VERSION"));

    // Print configuration card
    print_config(
        input,
        &config.target,
        &output_path,
        config.missing_threshold,
        config.gini_threshold,
        config.correlation_threshold,
    );

    // Load dataset and apply initial drops
    let (mut df, _initial_features, mut summary) =
        load_and_prepare_dataset(input, &config.columns_to_drop, cli.infer_schema_length)?;

    // Validate target and setup weights
    let weights = validate_target_and_weights(&df, &mut config, cli.no_confirm)?;

    // Run missing value analysis
    run_missing_analysis(&mut df, &config, &weights, &mut summary)?;

    // Run Gini/IV analysis
    run_gini_analysis(&df, &config, &cli, input, &weights, &mut summary)?;

    // Update df after Gini drops
    if !summary.dropped_gini.is_empty() {
        df = df.drop_many(&summary.dropped_gini);
    }

    // Run correlation analysis
    run_correlation_analysis(&mut df, &config, &weights, &mut summary)?;

    // Save results
    save_results(&mut df, &output_path, &mut summary)?;

    // Display summary and completion
    summary.display();
    print_completion();

    Ok(())
}

/// Setup configuration from CLI args or interactive menu
fn setup_configuration(
    cli: &Cli,
    input: &std::path::Path,
    output_path: &std::path::Path,
) -> Result<PipelineConfig> {
    // Build initial target mapping from CLI args if provided
    let cli_target_mapping = match (&cli.event_value, &cli.non_event_value) {
        (Some(event), Some(non_event)) => {
            Some(TargetMapping::new(event.clone(), non_event.clone()))
        }
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!("Both --event-value and --non-event-value must be provided together")
        }
        (None, None) => None,
    };

    if cli.no_confirm {
        // Skip interactive menu when --no-confirm is set
        let target = cli.target.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Target column is required when using --no-confirm. Use -t/--target to specify."
            )
        })?;

        Ok(PipelineConfig {
            target,
            missing_threshold: cli.missing_threshold,
            gini_threshold: cli.gini_threshold,
            gini_bins: cli.gini_bins,
            correlation_threshold: cli.correlation_threshold,
            columns_to_drop: cli.drop_columns.clone(),
            target_mapping: cli_target_mapping,
            weight_column: cli.weight_column.clone(),
        })
    } else {
        // Load column names for interactive selection
        let columns = get_column_names(input)?;

        // Show interactive config menu
        let config = Config {
            input: input.to_path_buf(),
            target: cli.target.clone(),
            output: output_path.to_path_buf(),
            missing_threshold: cli.missing_threshold,
            gini_threshold: cli.gini_threshold,
            correlation_threshold: cli.correlation_threshold,
            columns_to_drop: cli.drop_columns.clone(),
            target_mapping: cli_target_mapping,
            weight_column: cli.weight_column.clone(),
        };

        match run_config_menu(config, columns)? {
            ConfigResult::Proceed(cfg) => {
                let target = cfg.target.ok_or_else(|| {
                    anyhow::anyhow!("Target column must be selected before proceeding")
                })?;

                Ok(PipelineConfig {
                    target,
                    missing_threshold: cfg.missing_threshold,
                    gini_threshold: cfg.gini_threshold,
                    gini_bins: cli.gini_bins,
                    correlation_threshold: cfg.correlation_threshold,
                    columns_to_drop: cfg.columns_to_drop,
                    target_mapping: cfg.target_mapping,
                    weight_column: cfg.weight_column,
                })
            }
            ConfigResult::Quit => {
                println!("Cancelled by user.");
                std::process::exit(0);
            }
        }
    }
}

/// Load dataset and apply initial column drops
fn load_and_prepare_dataset(
    input: &std::path::Path,
    columns_to_drop: &[String],
    infer_schema_length: usize,
) -> Result<(polars::prelude::DataFrame, usize, ReductionSummary)> {
    let step_start = Instant::now();
    println!(); // Blank line before progress bar
    let (mut df, rows, cols, memory_mb) = load_dataset_with_progress(input, infer_schema_length)?;
    print_success("Dataset loaded");

    // Display statistics
    println!("\n    {} Dataset Statistics:", style("✧").cyan());
    println!("      Rows: {}", rows);
    println!("      Columns: {}", cols);
    println!("      Estimated memory: {:.2} MB", memory_mb);

    // Apply user-specified column drops
    let dropped_count = if !columns_to_drop.is_empty() {
        let column_names: Vec<String> = df
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
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

    Ok((df, initial_features, summary))
}

/// Validate target column and setup sample weights
fn validate_target_and_weights(
    df: &polars::prelude::DataFrame,
    config: &mut PipelineConfig,
    no_confirm: bool,
) -> Result<Vec<f64>> {
    // Verify target column exists
    let column_names: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    if !column_names.contains(&config.target) {
        anyhow::bail!(
            "Target column '{}' not found in dataset. Available columns: {:?}",
            config.target,
            column_names
        );
    }

    // Extract sample weights
    let weights = get_weights(df, config.weight_column.as_deref())?;
    if config.weight_column.is_some() {
        print_success(&format!(
            "Using weight column: '{}'",
            config.weight_column.as_ref().unwrap()
        ));
    }

    // Analyze target column to determine if mapping is needed
    if config.target_mapping.is_none() {
        match analyze_target_column(df, &config.target)? {
            TargetAnalysis::AlreadyBinary => {
                // No mapping needed - target is already 0/1
            }
            TargetAnalysis::NeedsMapping { unique_values } => {
                if no_confirm {
                    anyhow::bail!(
                        "Target column '{}' is not binary (0/1). Found {} unique values: {:?}\n\
                         Use --event-value and --non-event-value to specify which values map to 1 and 0.",
                        config.target,
                        unique_values.len(),
                        unique_values
                    );
                }

                // Show interactive selector
                println!();
                println!(
                    "   {} Target column '{}' is not binary (0/1)",
                    style("⚠").yellow(),
                    config.target
                );
                println!(
                    "     Found {} unique values: {:?}",
                    unique_values.len(),
                    &unique_values[..unique_values.len().min(5)]
                );
                if unique_values.len() > 5 {
                    println!("     ... and {} more", unique_values.len() - 5);
                }
                println!();

                match run_target_mapping_selector(unique_values)? {
                    TargetMappingResult::Selected(mapping) => {
                        println!(
                            "   {} Target mapping configured: '{}' → 1 (event), '{}' → 0 (non-event)",
                            style("✓").green(),
                            mapping.event_value,
                            mapping.non_event_value
                        );
                        config.target_mapping = Some(mapping);
                    }
                    TargetMappingResult::Cancelled => {
                        println!("Cancelled by user.");
                        std::process::exit(0);
                    }
                }
            }
        }
    } else {
        // Mapping was provided via CLI - display it
        let mapping = config.target_mapping.as_ref().unwrap();
        println!(
            "   {} Using target mapping: '{}' → 1, '{}' → 0",
            style("✓").green(),
            mapping.event_value,
            mapping.non_event_value
        );
    }

    Ok(weights)
}

/// Run missing value analysis
fn run_missing_analysis(
    df: &mut polars::prelude::DataFrame,
    config: &PipelineConfig,
    weights: &[f64],
    summary: &mut ReductionSummary,
) -> Result<()> {
    print_step_header(1, "Missing Value Analysis");

    let step_start = Instant::now();
    let spinner = create_spinner("Analyzing missing values...");
    let missing_ratios = analyze_missing_values(df, weights, config.weight_column.as_deref())?;
    let features_to_drop_missing =
        get_features_above_threshold(&missing_ratios, config.missing_threshold, &config.target);
    finish_with_success(&spinner, "Missing value analysis complete");

    if features_to_drop_missing.is_empty() {
        print_info("No features exceed the missing value threshold");
    } else {
        print_count(
            "feature(s) with high missing values",
            features_to_drop_missing.len(),
            Some(&format!("(>{:.1}%)", config.missing_threshold * 100.0)),
        );

        *df = df.clone().drop_many(&features_to_drop_missing);
        summary.add_missing_drops(features_to_drop_missing);
        print_success("Dropped features with high missing values");
    }

    let missing_elapsed = step_start.elapsed();
    summary.set_missing_time(missing_elapsed);
    print_step_time(missing_elapsed);

    Ok(())
}

/// Run Gini/IV analysis
fn run_gini_analysis(
    df: &polars::prelude::DataFrame,
    config: &PipelineConfig,
    cli: &Cli,
    input: &std::path::Path,
    weights: &[f64],
    summary: &mut ReductionSummary,
) -> Result<()> {
    print_step_header(2, "Univariate Gini Analysis");

    // Parse binning strategy
    let binning_strategy: BinningStrategy = cli
        .binning_strategy
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Parse solver config if solver is enabled
    let solver_config = if cli.use_solver {
        let monotonicity: MonotonicityConstraint = cli
            .monotonicity
            .parse()
            .map_err(|e: String| anyhow::anyhow!(e))?;

        Some(SolverConfig {
            timeout_seconds: cli.solver_timeout,
            gap_tolerance: cli.solver_gap,
            monotonicity,
            min_bin_samples: 5,
        })
    } else {
        None
    };

    let step_start = Instant::now();
    let gini_analyses = analyze_features_iv(
        df,
        &config.target,
        config.gini_bins,
        cli.prebins,
        config.target_mapping.as_ref(),
        binning_strategy,
        Some(cli.min_category_samples),
        Some(cli.cart_min_bin_pct),
        weights,
        config.weight_column.as_deref(),
        solver_config.as_ref(),
    )?;
    let features_to_drop_gini = get_low_gini_features(&gini_analyses, config.gini_threshold);

    // Export Gini analysis to JSON
    let gini_output_path = cli.gini_analysis_path().unwrap();
    let export_params = ExportParams {
        input_file: input.to_str().unwrap_or("unknown"),
        target_column: &config.target,
        weight_column: config.weight_column.as_deref(),
        binning_strategy,
        num_bins: config.gini_bins,
        gini_threshold: config.gini_threshold,
        min_category_samples: cli.min_category_samples,
        cart_min_bin_pct: if binning_strategy == BinningStrategy::Cart {
            Some(cli.cart_min_bin_pct)
        } else {
            None
        },
    };
    export_gini_analysis_enhanced(
        &gini_analyses,
        &features_to_drop_gini,
        &gini_output_path,
        &export_params,
    )?;
    print_success(&format!(
        "Gini analysis saved to {}",
        gini_output_path.display()
    ));

    if features_to_drop_gini.is_empty() {
        print_info("No features below Gini threshold");
    } else {
        print_count(
            "feature(s) with low Gini",
            features_to_drop_gini.len(),
            Some(&format!("(<{:.2})", config.gini_threshold)),
        );

        summary.add_gini_drops(features_to_drop_gini);
        print_success("Dropped low Gini features");
    }

    let gini_elapsed = step_start.elapsed();
    summary.set_gini_time(gini_elapsed);
    print_step_time(gini_elapsed);

    Ok(())
}

/// Run correlation analysis
fn run_correlation_analysis(
    df: &mut polars::prelude::DataFrame,
    config: &PipelineConfig,
    weights: &[f64],
    summary: &mut ReductionSummary,
) -> Result<()> {
    print_step_header(3, "Correlation Analysis");

    let step_start = Instant::now();
    let correlated_pairs = find_correlated_pairs_auto(
        df,
        config.correlation_threshold,
        weights,
        config.weight_column.as_deref(),
    )?;
    let features_to_drop_corr = select_features_to_drop(&correlated_pairs, &config.target);
    print_success("Correlation analysis complete");

    if correlated_pairs.is_empty() {
        print_info("No highly correlated feature pairs found");
    } else {
        print_count(
            "correlated pair(s)",
            correlated_pairs.len(),
            Some(&format!("(>{:.2})", config.correlation_threshold)),
        );
        println!(
            "      Dropping {} feature(s)",
            style(features_to_drop_corr.len()).yellow().bold()
        );

        *df = df.clone().drop_many(&features_to_drop_corr);
        summary.add_correlation_drops(features_to_drop_corr);
        print_success("Dropped highly correlated features");
    }

    let correlation_elapsed = step_start.elapsed();
    summary.set_correlation_time(correlation_elapsed);
    print_step_time(correlation_elapsed);

    Ok(())
}

/// Save results to output file
fn save_results(
    df: &mut polars::prelude::DataFrame,
    output_path: &std::path::Path,
    summary: &mut ReductionSummary,
) -> Result<()> {
    print_step_header(4, "Save Results");

    let step_start = Instant::now();
    let spinner = create_spinner("Writing output file...");
    save_dataset(df, output_path)?;
    finish_with_success(&spinner, &format!("Saved to {}", output_path.display()));

    let save_elapsed = step_start.elapsed();
    summary.set_save_time(save_elapsed);
    print_step_time(save_elapsed);

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
