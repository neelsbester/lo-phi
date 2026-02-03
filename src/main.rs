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
    run_config_menu, run_file_selector, run_target_mapping_selector, run_wizard, Cli, Commands,
    Config, ConfigResult, FileSelectResult, TargetMappingResult, WizardResult,
};
use pipeline::{
    analyze_features_iv, analyze_missing_values, analyze_target_column, find_correlated_pairs_auto,
    get_column_names, get_features_above_threshold, get_low_gini_features, get_weights,
    load_dataset_with_progress, select_features_to_drop, BinningStrategy, MonotonicityConstraint,
    SolverConfig, TargetAnalysis, TargetMapping,
};
use report::{
    export_gini_analysis_enhanced, export_reduction_report, export_reduction_report_csv,
    package_reduction_reports, ExportParams, ReductionReportBuilder, ReductionSummary,
    ReportBuilderParams,
};
use utils::{
    create_spinner, finish_with_success, print_banner, print_completion, print_config, print_count,
    print_info, print_step_header, print_step_time, print_success,
};

/// Configuration parameters for the reduction pipeline
struct PipelineConfig {
    /// Input file path
    input: std::path::PathBuf,
    /// Output file path
    output: std::path::PathBuf,
    target: String,
    missing_threshold: f64,
    gini_threshold: f64,
    gini_bins: usize,
    correlation_threshold: f64,
    columns_to_drop: Vec<String>,
    target_mapping: Option<TargetMapping>,
    weight_column: Option<String>,

    // Binning parameters
    binning_strategy: String,
    prebins: usize,
    cart_min_bin_pct: f64,
    min_category_samples: usize,

    // Solver options
    use_solver: bool,
    monotonicity: String,
    solver_timeout: u64,
    solver_gap: f64,

    // Data handling
    infer_schema_length: usize,
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

    // Setup configuration (handles wizard, manual, and CLI-only modes)
    let mut config = setup_configuration(&cli)?;
    let input = config.input.clone();
    let output_path = config.output.clone();

    // Print styled banner
    print_banner(env!("CARGO_PKG_VERSION"));

    // Print configuration card
    print_config(
        &input,
        &config.target,
        &output_path,
        config.missing_threshold,
        config.gini_threshold,
        config.correlation_threshold,
    );

    // Load dataset and apply initial drops
    let (mut df, _initial_features, mut summary) =
        load_and_prepare_dataset(&input, &config.columns_to_drop, config.infer_schema_length)?;

    // Validate target and setup weights
    let weights = validate_target_and_weights(&df, &mut config, cli.no_confirm)?;

    // Parse binning strategy for report
    let binning_strategy: BinningStrategy = config
        .binning_strategy
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Create report builder
    let mut report_builder = ReductionReportBuilder::new(ReportBuilderParams {
        input_file: input.to_string_lossy().to_string(),
        output_file: output_path.to_string_lossy().to_string(),
        target_column: config.target.clone(),
        weight_column: config.weight_column.clone(),
        binning_strategy: binning_strategy.to_string(),
        num_bins: config.gini_bins,
        missing_threshold: config.missing_threshold,
        gini_threshold: config.gini_threshold,
        correlation_threshold: config.correlation_threshold,
    });

    // Run missing value analysis
    let (missing_ratios, features_to_drop_missing) =
        run_missing_analysis(&mut df, &config, &weights, &mut summary)?;
    report_builder.set_missing_results(&missing_ratios, &features_to_drop_missing);

    // Run Gini/IV analysis
    let (gini_analyses, features_to_drop_gini) =
        run_gini_analysis(&df, &config, &input, &weights, &mut summary)?;
    report_builder.set_gini_results(&gini_analyses, &features_to_drop_gini);

    // Update df after Gini drops
    if !summary.dropped_gini.is_empty() {
        df = df.drop_many(&summary.dropped_gini);
    }

    // Run correlation analysis
    let (correlated_pairs, features_to_drop_corr) =
        run_correlation_analysis(&mut df, &config, &weights, &mut summary)?;
    report_builder.set_correlation_results(&correlated_pairs, &features_to_drop_corr);

    // Save results
    save_results(&mut df, &output_path, &mut summary)?;

    // Build and export reduction report
    report_builder.set_timing(&summary);
    let report = report_builder.build();
    let report_path = {
        let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        parent.join(format!("{}_reduction_report.json", stem))
    };
    export_reduction_report(&report, &report_path)?;

    // Also export CSV summary for easy viewing
    let csv_report_path = {
        let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        parent.join(format!("{}_reduction_report.csv", stem))
    };
    export_reduction_report_csv(&report, &csv_report_path)?;

    // Package all three reports into a zip file
    let gini_analysis_path = {
        let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        parent.join(format!("{}_gini_analysis.json", stem))
    };
    let zip_path = {
        let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        parent.join(format!("{}_reduction_report.zip", stem))
    };
    package_reduction_reports(
        &gini_analysis_path,
        &report_path,
        &csv_report_path,
        &zip_path,
    )?;

    print_success(&format!("Reduction report saved to {}", zip_path.display()));

    // Display summary and completion
    summary.display();
    print_completion();

    // Prompt before closing so double-click users can see results
    if !cli.no_confirm {
        println!("\n    Press Enter to exit...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }

    Ok(())
}

/// Resolve the input file path and derive the output path
fn resolve_paths(cli: &Cli) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let input = match cli.input() {
        Some(path) => path.clone(),
        None => {
            // Launch interactive file selector
            match run_file_selector()? {
                FileSelectResult::Selected(path) => path,
                FileSelectResult::Cancelled => {
                    println!("Cancelled by user.");
                    std::process::exit(0);
                }
            }
        }
    };

    let output_path = cli.output.clone().unwrap_or_else(|| {
        let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let extension = input
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("parquet");
        // SAS7BDAT input defaults to Parquet output (no SAS7BDAT write support)
        let output_ext = if extension.eq_ignore_ascii_case("sas7bdat") {
            "parquet"
        } else {
            extension
        };
        parent.join(format!("{}_reduced.{}", stem, output_ext))
    });

    Ok((input, output_path))
}

/// Convert a Config to PipelineConfig
fn config_to_pipeline_config(cfg: Config) -> Result<PipelineConfig> {
    let target = cfg
        .target
        .ok_or_else(|| anyhow::anyhow!("Target column must be selected before proceeding"))?;

    Ok(PipelineConfig {
        input: cfg.input,
        output: cfg.output,
        target,
        missing_threshold: cfg.missing_threshold,
        gini_threshold: cfg.gini_threshold,
        gini_bins: cfg.gini_bins,
        correlation_threshold: cfg.correlation_threshold,
        columns_to_drop: cfg.columns_to_drop,
        target_mapping: cfg.target_mapping,
        weight_column: cfg.weight_column,
        binning_strategy: cfg.binning_strategy,
        prebins: cfg.prebins,
        cart_min_bin_pct: cfg.cart_min_bin_pct,
        min_category_samples: cfg.min_category_samples,
        use_solver: cfg.use_solver,
        monotonicity: cfg.monotonicity,
        solver_timeout: cfg.solver_timeout,
        solver_gap: cfg.solver_gap,
        infer_schema_length: cfg.infer_schema_length,
    })
}

/// Setup configuration from CLI args, wizard, or interactive dashboard
fn setup_configuration(cli: &Cli) -> Result<PipelineConfig> {
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

    // Branch 1: --no-confirm (CLI-only, existing behavior)
    if cli.no_confirm {
        let (input, output_path) = resolve_paths(cli)?;
        let target = cli.target.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Target column is required when using --no-confirm. Use -t/--target to specify."
            )
        })?;

        return Ok(PipelineConfig {
            input,
            output: output_path,
            target,
            missing_threshold: cli.missing_threshold,
            gini_threshold: cli.gini_threshold,
            gini_bins: cli.gini_bins,
            correlation_threshold: cli.correlation_threshold,
            columns_to_drop: cli.drop_columns.clone(),
            target_mapping: cli_target_mapping,
            weight_column: cli.weight_column.clone(),
            binning_strategy: cli.binning_strategy.clone(),
            prebins: cli.prebins,
            cart_min_bin_pct: cli.cart_min_bin_pct,
            min_category_samples: cli.min_category_samples,
            use_solver: cli.use_solver,
            monotonicity: cli.monotonicity.clone(),
            solver_timeout: cli.solver_timeout,
            solver_gap: cli.solver_gap,
            infer_schema_length: cli.infer_schema_length,
        });
    }

    // Branch 2: --manual (Dashboard, existing behavior)
    if cli.manual {
        let (input, output_path) = resolve_paths(cli)?;
        let mut current_input = input;
        let mut columns = get_column_names(&current_input)?;

        let mut config = Config {
            input: current_input.clone(),
            target: cli.target.clone(),
            output: output_path,
            missing_threshold: cli.missing_threshold,
            gini_threshold: cli.gini_threshold,
            correlation_threshold: cli.correlation_threshold,
            columns_to_drop: cli.drop_columns.clone(),
            target_mapping: cli_target_mapping,
            weight_column: cli.weight_column.clone(),
            binning_strategy: cli.binning_strategy.clone(),
            gini_bins: cli.gini_bins,
            prebins: cli.prebins,
            cart_min_bin_pct: cli.cart_min_bin_pct,
            min_category_samples: cli.min_category_samples,
            use_solver: cli.use_solver,
            monotonicity: cli.monotonicity.clone(),
            solver_timeout: cli.solver_timeout,
            solver_gap: cli.solver_gap,
            infer_schema_length: cli.infer_schema_length,
        };

        loop {
            match run_config_menu(config.clone(), columns.clone())? {
                ConfigResult::Proceed(boxed_cfg) => {
                    return config_to_pipeline_config(*boxed_cfg);
                }
                ConfigResult::Convert(boxed_cfg) => {
                    let cfg = *boxed_cfg;
                    // Run file format conversion
                    cli::convert::run_convert(
                        &cfg.input,
                        None, // Auto-generate output path
                        cfg.infer_schema_length,
                        true, // Use fast mode
                    )?;

                    // Determine the converted file's path based on input format
                    let input_ext = cfg
                        .input
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    let converted_ext = if input_ext == "parquet" {
                        "csv"
                    } else {
                        "parquet"
                    };
                    let converted_path = cfg.input.with_extension(converted_ext);
                    current_input = converted_path.clone();
                    columns = get_column_names(&current_input)?;

                    let new_output = {
                        let parent = current_input
                            .parent()
                            .unwrap_or_else(|| std::path::Path::new("."));
                        let stem = current_input
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("output");
                        let out_ext = current_input
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("parquet");
                        parent.join(format!("{}_reduced.{}", stem, out_ext))
                    };

                    config = Config {
                        input: current_input.clone(),
                        output: new_output,
                        target: cfg.target,
                        missing_threshold: cfg.missing_threshold,
                        gini_threshold: cfg.gini_threshold,
                        correlation_threshold: cfg.correlation_threshold,
                        columns_to_drop: cfg.columns_to_drop,
                        target_mapping: cfg.target_mapping,
                        weight_column: cfg.weight_column,
                        binning_strategy: cfg.binning_strategy,
                        gini_bins: cfg.gini_bins,
                        prebins: cfg.prebins,
                        cart_min_bin_pct: cfg.cart_min_bin_pct,
                        min_category_samples: cfg.min_category_samples,
                        use_solver: cfg.use_solver,
                        monotonicity: cfg.monotonicity,
                        solver_timeout: cfg.solver_timeout,
                        solver_gap: cfg.solver_gap,
                        infer_schema_length: cfg.infer_schema_length,
                    };

                    println!("\nPress any key to continue...");
                    let _ = std::io::stdin().read_line(&mut String::new());
                }
                ConfigResult::Quit => {
                    println!("Cancelled by user.");
                    std::process::exit(0);
                }
            }
        }
    }

    // Branch 3: Default (Wizard, new behavior)
    match run_wizard(cli)? {
        WizardResult::RunReduction(boxed_cfg) => config_to_pipeline_config(*boxed_cfg),
        WizardResult::RunConversion(conversion_config) => {
            cli::convert::run_convert(
                &conversion_config.input,
                Some(&conversion_config.output),
                conversion_config.infer_schema_length,
                conversion_config.fast,
            )?;
            println!(
                "Conversion complete: {}",
                conversion_config.output.display()
            );
            std::process::exit(0);
        }
        WizardResult::Quit => {
            println!("Cancelled by user.");
            std::process::exit(0);
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
/// Returns (missing_ratios, features_dropped) for report generation
#[allow(clippy::type_complexity)]
fn run_missing_analysis(
    df: &mut polars::prelude::DataFrame,
    config: &PipelineConfig,
    weights: &[f64],
    summary: &mut ReductionSummary,
) -> Result<(Vec<(String, f64)>, Vec<String>)> {
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
        summary.add_missing_drops(features_to_drop_missing.clone());
        print_success("Dropped features with high missing values");
    }

    let missing_elapsed = step_start.elapsed();
    summary.set_missing_time(missing_elapsed);
    print_step_time(missing_elapsed);

    Ok((missing_ratios, features_to_drop_missing))
}

/// Run Gini/IV analysis
/// Returns (gini_analyses, features_dropped) for report generation
fn run_gini_analysis(
    df: &polars::prelude::DataFrame,
    config: &PipelineConfig,
    input: &std::path::Path,
    weights: &[f64],
    summary: &mut ReductionSummary,
) -> Result<(Vec<pipeline::IvAnalysis>, Vec<String>)> {
    print_step_header(2, "Univariate Gini Analysis");

    // Parse binning strategy
    let binning_strategy: BinningStrategy = config
        .binning_strategy
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Parse solver config if solver is enabled
    let solver_config = if config.use_solver {
        let monotonicity: MonotonicityConstraint = config
            .monotonicity
            .parse()
            .map_err(|e: String| anyhow::anyhow!(e))?;

        Some(SolverConfig {
            timeout_seconds: config.solver_timeout,
            gap_tolerance: config.solver_gap,
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
        config.prebins,
        config.target_mapping.as_ref(),
        binning_strategy,
        Some(config.min_category_samples),
        Some(config.cart_min_bin_pct),
        weights,
        config.weight_column.as_deref(),
        solver_config.as_ref(),
    )?;
    let features_to_drop_gini = get_low_gini_features(&gini_analyses, config.gini_threshold);

    // Export Gini analysis to JSON
    let gini_output_path = {
        let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        parent.join(format!("{}_gini_analysis.json", stem))
    };
    let export_params = ExportParams {
        input_file: input.to_str().unwrap_or("unknown"),
        target_column: &config.target,
        weight_column: config.weight_column.as_deref(),
        binning_strategy,
        num_bins: config.gini_bins,
        gini_threshold: config.gini_threshold,
        min_category_samples: config.min_category_samples,
        cart_min_bin_pct: if binning_strategy == BinningStrategy::Cart {
            Some(config.cart_min_bin_pct)
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

    if features_to_drop_gini.is_empty() {
        print_info("No features below Gini threshold");
    } else {
        print_count(
            "feature(s) with low Gini",
            features_to_drop_gini.len(),
            Some(&format!("(<{:.2})", config.gini_threshold)),
        );

        summary.add_gini_drops(features_to_drop_gini.clone());
        print_success("Dropped low Gini features");
    }

    let gini_elapsed = step_start.elapsed();
    summary.set_gini_time(gini_elapsed);
    print_step_time(gini_elapsed);

    Ok((gini_analyses, features_to_drop_gini))
}

/// Run correlation analysis
/// Returns (correlated_pairs, features_dropped) for report generation
fn run_correlation_analysis(
    df: &mut polars::prelude::DataFrame,
    config: &PipelineConfig,
    weights: &[f64],
    summary: &mut ReductionSummary,
) -> Result<(Vec<pipeline::CorrelatedPair>, Vec<String>)> {
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
        summary.add_correlation_drops(features_to_drop_corr.clone());
        print_success("Dropped highly correlated features");
    }

    let correlation_elapsed = step_start.elapsed();
    summary.set_correlation_time(correlation_elapsed);
    print_step_time(correlation_elapsed);

    Ok((correlated_pairs, features_to_drop_corr))
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
