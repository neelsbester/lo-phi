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

use cli::{run_config_menu, Cli, Commands, Config, ConfigResult};
use pipeline::{
    analyze_features_iv, analyze_missing_values, find_correlated_pairs, get_column_names,
    get_features_above_threshold, get_low_gini_features, load_dataset_with_progress,
    select_features_to_drop,
};
use report::ReductionSummary;
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
            } => cli::convert::run_convert(input, output.as_deref(), *infer_schema_length),
        };
    }

    // Main reduce pipeline - require input
    let input = cli.input().ok_or_else(|| {
        anyhow::anyhow!("Input file is required. Use -i/--input to specify a file.")
    })?;
    
    // Derive output path from input if not provided
    let output_path = cli.output_path().unwrap();

    // Determine final config values - either from interactive menu or CLI defaults
    let (target, missing_threshold, gini_threshold, gini_bins, correlation_threshold) = if cli.no_confirm {
        // Skip interactive menu when --no-confirm is set
        // Target is required in non-interactive mode
        let target = cli.target.clone().ok_or_else(|| {
            anyhow::anyhow!("Target column is required when using --no-confirm. Use -t/--target to specify.")
        })?;
        (target, cli.missing_threshold, cli.gini_threshold, cli.gini_bins, cli.correlation_threshold)
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
        };

        match run_config_menu(config, columns)? {
            ConfigResult::Proceed(cfg) => {
                let target = cfg.target.ok_or_else(|| {
                    anyhow::anyhow!("Target column must be selected before proceeding")
                })?;
                (target, cfg.missing_threshold, cfg.gini_threshold, cli.gini_bins, cfg.correlation_threshold)
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

    let initial_features = cols;
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

    // Step 2: Missing value analysis
    // NOTE: All analysis functions now take &DataFrame to avoid repeated collection
    print_step_header(1, "Missing Value Analysis");

    let step_start = Instant::now();
    let spinner = create_spinner("Analyzing missing values...");
    let missing_ratios = analyze_missing_values(&df)?;
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

    let step_start = Instant::now();
    let gini_analyses = analyze_features_iv(&df, &target, gini_bins)?;
    let features_to_drop_gini = get_low_gini_features(&gini_analyses, gini_threshold);

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
    let spinner = create_spinner("Calculating correlations...");
    let correlated_pairs = find_correlated_pairs(&df, correlation_threshold)?;
    let features_to_drop_corr = select_features_to_drop(&correlated_pairs, &target);
    finish_with_success(&spinner, "Correlation analysis complete");

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
