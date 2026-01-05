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
use polars::prelude::IntoLazy;

use cli::{run_config_menu, Args, Config, ConfigResult};
use pipeline::{
    analyze_features_iv, analyze_missing_values, drop_correlated_features,
    drop_high_missing_features, drop_low_gini_features, find_correlated_pairs, get_column_names,
    get_features_above_threshold, get_low_gini_features, load_and_collect, load_dataset,
    select_features_to_drop,
};
use report::ReductionSummary;
use utils::{
    create_spinner, finish_with_success, print_banner, print_completion, print_config,
    print_count, print_info, print_step_header, print_success,
};

fn main() -> Result<()> {
    let args = Args::parse();
    
    // Derive output path from input if not provided
    let output_path = args.output_path();

    // Determine final config values - either from interactive menu or CLI defaults
    let (target, missing_threshold, gini_threshold, gini_bins, correlation_threshold) = if args.no_confirm {
        // Skip interactive menu when --no-confirm is set
        // Target is required in non-interactive mode
        let target = args.target.clone().ok_or_else(|| {
            anyhow::anyhow!("Target column is required when using --no-confirm. Use -t/--target to specify.")
        })?;
        (target, args.missing_threshold, args.gini_threshold, args.gini_bins, args.correlation_threshold)
    } else {
        // Load column names for interactive selection
        let columns = get_column_names(&args.input)?;
        
        // Show interactive config menu
        let config = Config {
            input: args.input.clone(),
            target: args.target.clone(),
            output: output_path.clone(),
            missing_threshold: args.missing_threshold,
            gini_threshold: args.gini_threshold,
            correlation_threshold: args.correlation_threshold,
        };

        match run_config_menu(config, columns)? {
            ConfigResult::Proceed(cfg) => {
                let target = cfg.target.ok_or_else(|| {
                    anyhow::anyhow!("Target column must be selected before proceeding")
                })?;
                (target, cfg.missing_threshold, cfg.gini_threshold, args.gini_bins, cfg.correlation_threshold)
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
        &args.input,
        &target,
        &output_path,
        missing_threshold,
        gini_threshold,
        correlation_threshold,
    );

    // Step 1: Load dataset
    let step_start = Instant::now();
    let spinner = create_spinner("Loading dataset...");
    let lf = load_dataset(&args.input, args.infer_schema_length)?;
    let (df, rows, cols, memory_mb) = load_and_collect(lf)?;
    finish_with_success(&spinner, "Dataset loaded");

    // Display statistics (instant since data is already collected)
    println!("\n    {} Dataset Statistics:", style("✧").cyan());
    println!("      Rows: {}", rows);
    println!("      Columns: {}", cols);
    println!("      Estimated memory: {:.2} MB", memory_mb);

    // Convert back to LazyFrame for pipeline operations
    let mut lf = df.lazy();
    let initial_features = cols;
    let mut summary = ReductionSummary::new(initial_features);
    summary.set_load_time(step_start.elapsed());

    // Verify target column exists
    let schema = lf.collect_schema()?;
    if !schema.contains(&target) {
        anyhow::bail!(
            "Target column '{}' not found in dataset. Available columns: {:?}",
            target,
            schema.iter_names().collect::<Vec<_>>()
        );
    }

    // Step 2: Missing value analysis
    print_step_header(1, "Missing Value Analysis");

    let step_start = Instant::now();
    let spinner = create_spinner("Analyzing missing values...");
    let missing_ratios = analyze_missing_values(&lf)?;
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

        lf = drop_high_missing_features(lf, &features_to_drop_missing);
        summary.add_missing_drops(features_to_drop_missing);
        print_success("Dropped features with high missing values");
    }
    summary.set_missing_time(step_start.elapsed());

    // Step 2: Univariate Gini Analysis
    print_step_header(2, "Univariate Gini Analysis");

    let step_start = Instant::now();
    let gini_analyses = analyze_features_iv(&lf, &target, gini_bins)?;
    let features_to_drop_gini = get_low_gini_features(&gini_analyses, gini_threshold);

    if features_to_drop_gini.is_empty() {
        print_info("No features below Gini threshold");
    } else {
        print_count(
            "feature(s) with low Gini",
            features_to_drop_gini.len(),
            Some(&format!("(<{:.2})", gini_threshold)),
        );

        lf = drop_low_gini_features(lf, &features_to_drop_gini);
        summary.add_gini_drops(features_to_drop_gini);
        print_success("Dropped low Gini features");
    }
    summary.set_gini_time(step_start.elapsed());

    // Step 3: Correlation analysis
    print_step_header(3, "Correlation Analysis");

    let step_start = Instant::now();
    let spinner = create_spinner("Calculating correlations...");
    let correlated_pairs = find_correlated_pairs(&lf, correlation_threshold)?;
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

        lf = drop_correlated_features(lf, &features_to_drop_corr);
        summary.add_correlation_drops(features_to_drop_corr);
        print_success("Dropped highly correlated features");
    }
    summary.set_correlation_time(step_start.elapsed());

    // Step 4: Save output
    print_step_header(4, "Save Results");

    let step_start = Instant::now();
    let spinner = create_spinner("Writing output file...");
    save_dataset(&lf, &output_path)?;
    finish_with_success(
        &spinner,
        &format!("Saved to {}", output_path.display()),
    );
    summary.set_save_time(step_start.elapsed());

    // Display summary
    summary.display();

    // Final completion message
    print_completion();

    Ok(())
}

/// Save dataset to file (CSV or Parquet based on extension)
fn save_dataset(lf: &polars::prelude::LazyFrame, path: &std::path::Path) -> Result<()> {
    use anyhow::Context;
    use polars::prelude::*;

    let mut df = lf.clone().collect()?;

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
                .finish(&mut df)
                .with_context(|| format!("Failed to write CSV file: {}", path.display()))?;
        }
        "parquet" => {
            let file = std::fs::File::create(path)
                .with_context(|| format!("Failed to create output file: {}", path.display()))?;
            ParquetWriter::new(file)
                .finish(&mut df)
                .with_context(|| format!("Failed to write Parquet file: {}", path.display()))?;
        }
        _ => anyhow::bail!(
            "Unsupported output format: {}. Supported formats: csv, parquet",
            extension
        ),
    }

    Ok(())
}
