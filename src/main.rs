//! Lo-phi: Feature Reduction CLI Tool
//!
//! A command-line tool for reducing features in datasets using
//! missing value analysis and correlation-based reduction.

mod cli;
mod pipeline;
mod report;
mod utils;

use anyhow::Result;
use clap::Parser;
use console::style;

use cli::{Args, confirm_drop_features};
use pipeline::{
    analyze_missing_values, display_dataset_stats, drop_correlated_features,
    drop_high_missing_features, find_correlated_pairs, get_features_above_threshold,
    load_dataset, select_features_to_drop,
};
use report::ReductionSummary;
use utils::{
    create_spinner, finish_with_success, print_banner, print_completion, print_config,
    print_count, print_info, print_skip, print_step_header, print_success,
};

fn main() -> Result<()> {
    let args = Args::parse();

    // Print styled banner
    print_banner(env!("CARGO_PKG_VERSION"));

    // Print configuration card
    print_config(
        &args.input,
        &args.target,
        &args.output,
        args.missing_threshold,
        args.correlation_threshold,
    );

    // Step 1: Load dataset
    let spinner = create_spinner("Loading dataset...");
    let mut lf = load_dataset(&args.input)?;
    finish_with_success(&spinner, "Dataset loaded");

    // Display initial statistics
    display_dataset_stats(&lf)?;

    // Get initial feature count
    let initial_schema = lf.collect_schema()?;
    let initial_features = initial_schema.len();
    let mut summary = ReductionSummary::new(initial_features);

    // Verify target column exists
    if !initial_schema.contains(&args.target) {
        anyhow::bail!(
            "Target column '{}' not found in dataset. Available columns: {:?}",
            args.target,
            initial_schema.iter_names().collect::<Vec<_>>()
        );
    }

    // Step 2: Missing value analysis
    print_step_header(1, "Missing Value Analysis");

    let spinner = create_spinner("Analyzing missing values...");
    let missing_ratios = analyze_missing_values(&lf)?;
    let features_to_drop_missing = get_features_above_threshold(
        &missing_ratios,
        args.missing_threshold,
        &args.target,
    );
    finish_with_success(&spinner, "Missing value analysis complete");

    if features_to_drop_missing.is_empty() {
        print_info("No features exceed the missing value threshold");
    } else {
        print_count(
            "feature(s) with high missing values",
            features_to_drop_missing.len(),
            Some(&format!("(>{:.1}%)", args.missing_threshold * 100.0)),
        );

        let should_drop = args.no_confirm
            || confirm_drop_features(features_to_drop_missing.len(), "missing value")?;

        if should_drop {
            lf = drop_high_missing_features(lf, &features_to_drop_missing);
            summary.add_missing_drops(features_to_drop_missing);
            print_success("Dropped features with high missing values");
        } else {
            print_skip("Skipped missing value reduction");
        }
    }

    // Step 3: Correlation analysis
    print_step_header(2, "Correlation Analysis");

    let spinner = create_spinner("Calculating correlations...");
    let correlated_pairs = find_correlated_pairs(&lf, args.correlation_threshold)?;
    let features_to_drop_corr = select_features_to_drop(&correlated_pairs, &args.target);
    finish_with_success(&spinner, "Correlation analysis complete");

    if correlated_pairs.is_empty() {
        print_info("No highly correlated feature pairs found");
    } else {
        print_count(
            "correlated pair(s)",
            correlated_pairs.len(),
            Some(&format!("(>{:.2})", args.correlation_threshold)),
        );
        println!(
            "      Recommended to drop {} feature(s)",
            style(features_to_drop_corr.len()).yellow().bold()
        );

        let should_drop = args.no_confirm
            || confirm_drop_features(features_to_drop_corr.len(), "correlation")?;

        if should_drop {
            lf = drop_correlated_features(lf, &features_to_drop_corr);
            summary.add_correlation_drops(features_to_drop_corr);
            print_success("Dropped highly correlated features");
        } else {
            print_skip("Skipped correlation reduction");
        }
    }

    // Step 4: Save output
    print_step_header(3, "Save Results");

    let spinner = create_spinner("Writing output file...");
    save_dataset(&lf, &args.output)?;
    finish_with_success(
        &spinner,
        &format!("Saved to {}", args.output.display()),
    );

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
