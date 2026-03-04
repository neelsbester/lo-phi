//! Lo-phi: Feature Reduction CLI Tool
//!
//! A command-line tool for reducing features in datasets using
//! missing value analysis and correlation-based reduction.

mod cli;
mod pipeline;
mod report;
mod utils;

use std::io::Stdout;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use console::style;
use ratatui::{backend::CrosstermBackend, Terminal};

use cli::{
    run_config_menu_keep_tui, run_file_selector, run_target_mapping_selector, run_wizard_keep_tui,
    Cli, Commands, Config, ConfigResult, FileSelectResult, TargetMappingResult, WizardResult,
};
use pipeline::{
    analyze_features_iv, analyze_features_iv_with_progress, analyze_missing_values,
    analyze_target_column, create_progress_channel, find_correlated_pairs_auto,
    find_correlated_pairs_auto_with_progress, get_column_names, get_features_above_threshold,
    get_low_gini_features, get_weights, load_dataset_with_progress,
    load_dataset_with_progress_channel, select_features_to_drop, BinningStrategy, FeatureMetadata,
    FeatureToDrop, MonotonicityConstraint, PipelineStage, ProgressEvent, ProgressSender,
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

/// Derive an output path from an input path by appending a suffix and changing the extension.
///
/// For example, `derive_output_path("/data/foo.csv", "reduced", "parquet")` returns
/// `/data/foo_reduced.parquet`.
fn derive_output_path(input: &std::path::Path, suffix: &str, ext: &str) -> std::path::PathBuf {
    let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    parent.join(format!("{}_{}.{}", stem, suffix, ext))
}

/// Configuration parameters for the reduction pipeline
#[derive(Clone)]
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

    // --no-confirm: pure CLI mode, existing indicatif-based output
    if cli.no_confirm {
        let Some(config) = setup_configuration_no_tui(&cli)? else {
            return Ok(());
        };
        return run_pipeline_no_tui(config);
    }

    // Interactive mode (wizard or dashboard): keep TUI alive for progress overlay
    let (pipeline_config, terminal_opt) = setup_configuration_interactive(&cli)?;
    let Some(pipeline_config) = pipeline_config else {
        return Ok(());
    };

    if let Some(mut terminal) = terminal_opt {
        // TUI is still active — run pipeline with in-TUI progress overlay
        run_pipeline_with_tui(pipeline_config, &mut terminal)?;
        // Tear down after overlay exits
        cli::wizard::teardown_terminal();
    } else {
        // Interactive setup completed but terminal was torn down (conversion path, etc.)
        run_pipeline_no_tui(pipeline_config)?;
    }

    Ok(())
}

// ============================================================================
// Configuration setup helpers
// ============================================================================

/// Resolve the input file path and derive the output path.
/// Returns `Ok(None)` if the user cancelled file selection.
fn resolve_paths(cli: &Cli) -> Result<Option<(std::path::PathBuf, std::path::PathBuf)>> {
    let input = match cli.input() {
        Some(path) => path.clone(),
        None => {
            // Launch interactive file selector
            match run_file_selector()? {
                FileSelectResult::Selected(path) => path,
                FileSelectResult::Cancelled => {
                    println!("Cancelled by user.");
                    return Ok(None);
                }
            }
        }
    };

    let output_path = cli.output.clone().unwrap_or_else(|| {
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
        derive_output_path(&input, "reduced", output_ext)
    });

    Ok(Some((input, output_path)))
}

/// Convert a Config to PipelineConfig
fn config_to_pipeline_config(cfg: Config) -> Result<Option<PipelineConfig>> {
    let target = cfg
        .target
        .ok_or_else(|| anyhow::anyhow!("Target column must be selected before proceeding"))?;

    Ok(Some(PipelineConfig {
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
    }))
}

/// Setup configuration for `--no-confirm` mode (pure CLI, no TUI involved).
fn setup_configuration_no_tui(cli: &Cli) -> Result<Option<PipelineConfig>> {
    let Some((input, output_path)) = resolve_paths(cli)? else {
        return Ok(None);
    };
    let target = cli.target.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "Target column is required when using --no-confirm. Use -t/--target to specify."
        )
    })?;

    let cli_target_mapping = match (&cli.event_value, &cli.non_event_value) {
        (Some(event), Some(non_event)) => {
            Some(TargetMapping::new(event.clone(), non_event.clone()))
        }
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!("Both --event-value and --non-event-value must be provided together")
        }
        (None, None) => None,
    };

    Ok(Some(PipelineConfig {
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
    }))
}

/// Setup configuration in interactive mode (wizard or dashboard).
///
/// Returns `(Option<PipelineConfig>, Option<Terminal>)`.
/// When `Terminal` is `Some`, the TUI is still active and the caller must
/// display the progress overlay and then call `teardown_terminal()`.
#[allow(clippy::type_complexity)]
fn setup_configuration_interactive(
    cli: &Cli,
) -> Result<(
    Option<PipelineConfig>,
    Option<Terminal<CrosstermBackend<Stdout>>>,
)> {
    let cli_target_mapping = match (&cli.event_value, &cli.non_event_value) {
        (Some(event), Some(non_event)) => {
            Some(TargetMapping::new(event.clone(), non_event.clone()))
        }
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!("Both --event-value and --non-event-value must be provided together")
        }
        (None, None) => None,
    };

    // Branch: --manual (Dashboard)
    if cli.manual {
        let Some((input, output_path)) = resolve_paths(cli)? else {
            return Ok((None, None));
        };
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
            match run_config_menu_keep_tui(config.clone(), columns.clone())? {
                (ConfigResult::Proceed(boxed_cfg), terminal_opt) => {
                    let cfg_opt = config_to_pipeline_config(*boxed_cfg)?;
                    return Ok((cfg_opt, terminal_opt));
                }
                (ConfigResult::Convert(boxed_cfg), _) => {
                    let cfg = *boxed_cfg;
                    // Run file format conversion (TUI is torn down at this point)
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
                        let out_ext = current_input
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("parquet");
                        derive_output_path(&current_input, "reduced", out_ext)
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
                (ConfigResult::Quit, _) => {
                    println!("Cancelled by user.");
                    return Ok((None, None));
                }
            }
        }
    }

    // Default: Wizard
    match run_wizard_keep_tui(cli)? {
        (WizardResult::RunReduction(boxed_cfg), terminal_opt) => {
            let cfg_opt = config_to_pipeline_config(*boxed_cfg)?;
            Ok((cfg_opt, terminal_opt))
        }
        (WizardResult::RunConversion(conversion_config), _) => {
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
            Ok((None, None))
        }
        (WizardResult::Quit, _) => {
            println!("Cancelled by user.");
            Ok((None, None))
        }
    }
}

// ============================================================================
// Pipeline execution: TUI overlay path
// ============================================================================

/// Run the full reduction pipeline while the TUI is still active.
///
/// Spawns the pipeline in a background thread and drives a progress overlay
/// in the foreground event loop.
fn run_pipeline_with_tui(
    config: PipelineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<()> {
    let (tx, rx) = create_progress_channel();
    let config_clone = config.clone();

    let handle = std::thread::spawn(move || run_pipeline_bg(config_clone, tx));

    // Drive the TUI overlay until complete or user aborts
    cli::progress_overlay::run_progress_overlay(terminal, rx)?;

    // Collect pipeline result (propagate errors)
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("Pipeline thread panicked"))??;

    Ok(())
}

/// Run the full reduction pipeline, sending progress events over `tx`.
/// This is designed to run in a background thread.
fn run_pipeline_bg(mut config: PipelineConfig, tx: ProgressSender) -> Result<()> {
    let input = config.input.clone();
    let output_path = config.output.clone();
    let pipeline_start = Instant::now();

    // ── Stage: Loading ────────────────────────────────────────────────────
    tx.send(ProgressEvent::stage_start(
        PipelineStage::Loading,
        "Loading dataset",
    ))
    .ok();

    let stage_start = Instant::now();
    let (mut df, _initial_features, mut summary) = load_and_prepare_dataset_with_tx(
        &input,
        &config.columns_to_drop,
        config.infer_schema_length,
        &tx,
    )?;

    tx.send(ProgressEvent::stage_complete(
        PipelineStage::Loading,
        "Dataset loaded",
        stage_start.elapsed(),
    ))
    .ok();

    // ── Stage: Validating ─────────────────────────────────────────────────
    tx.send(ProgressEvent::stage_start(
        PipelineStage::Validating,
        "Validating target",
    ))
    .ok();

    let stage_start = Instant::now();
    let weights = validate_target_and_weights_headless(&df, &mut config)?;

    tx.send(ProgressEvent::stage_complete(
        PipelineStage::Validating,
        "Target validated",
        stage_start.elapsed(),
    ))
    .ok();

    // Parse binning strategy
    let binning_strategy: BinningStrategy = config
        .binning_strategy
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Build report
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

    // ── Stage: Missing ────────────────────────────────────────────────────
    tx.send(ProgressEvent::stage_start(
        PipelineStage::MissingAnalysis,
        "Missing value analysis",
    ))
    .ok();

    let stage_start = Instant::now();
    let (missing_ratios, features_to_drop_missing) =
        run_missing_analysis_bg(&mut df, &config, &weights, &mut summary)?;
    report_builder.set_missing_results(&missing_ratios, &features_to_drop_missing);

    tx.send(ProgressEvent::stage_complete(
        PipelineStage::MissingAnalysis,
        "Missing value analysis complete",
        stage_start.elapsed(),
    ))
    .ok();

    // ── Stage: Gini/IV ────────────────────────────────────────────────────
    tx.send(ProgressEvent::stage_start(
        PipelineStage::GiniAnalysis,
        "Gini/IV analysis",
    ))
    .ok();

    let stage_start = Instant::now();
    let (gini_analyses, features_to_drop_gini) =
        run_gini_analysis_bg(&df, &config, &input, &weights, &mut summary, &tx)?;
    report_builder.set_gini_results(&gini_analyses, &features_to_drop_gini);

    if !summary.dropped_gini.is_empty() {
        df = df.drop_many(&summary.dropped_gini);
    }

    // Build metadata maps for IV-first correlation drop logic
    let (feature_metadata, feature_types) =
        build_correlation_metadata(&gini_analyses, &missing_ratios);

    tx.send(ProgressEvent::stage_complete(
        PipelineStage::GiniAnalysis,
        "Gini/IV analysis complete",
        stage_start.elapsed(),
    ))
    .ok();

    // ── Stage: Correlation ────────────────────────────────────────────────
    tx.send(ProgressEvent::stage_start(
        PipelineStage::CorrelationAnalysis,
        "Correlation analysis",
    ))
    .ok();

    let stage_start = Instant::now();
    let (correlated_pairs, features_to_drop_corr) = run_correlation_analysis_bg(
        &mut df,
        &config,
        &weights,
        &mut summary,
        &tx,
        &feature_metadata,
        &feature_types,
    )?;
    report_builder.set_correlation_results(&correlated_pairs, &features_to_drop_corr);

    tx.send(ProgressEvent::stage_complete(
        PipelineStage::CorrelationAnalysis,
        "Correlation analysis complete",
        stage_start.elapsed(),
    ))
    .ok();

    // ── Stage: Saving ─────────────────────────────────────────────────────
    tx.send(ProgressEvent::stage_start(
        PipelineStage::Saving,
        "Saving results",
    ))
    .ok();

    let stage_start = Instant::now();
    save_results_bg(&mut df, &output_path, &mut summary)?;

    tx.send(ProgressEvent::stage_complete(
        PipelineStage::Saving,
        "Results saved",
        stage_start.elapsed(),
    ))
    .ok();

    // ── Stage: Reports ────────────────────────────────────────────────────
    tx.send(ProgressEvent::stage_start(
        PipelineStage::Reports,
        "Generating reports",
    ))
    .ok();

    let stage_start = Instant::now();
    report_builder.set_timing(&summary);
    let report = report_builder.build();

    let report_path = derive_output_path(&input, "reduction_report", "json");
    export_reduction_report(&report, &report_path)?;

    let csv_report_path = derive_output_path(&input, "reduction_report", "csv");
    export_reduction_report_csv(&report, &csv_report_path)?;

    let gini_analysis_path = derive_output_path(&input, "gini_analysis", "json");
    let zip_path = derive_output_path(&input, "reduction_report", "zip");
    package_reduction_reports(
        &gini_analysis_path,
        &report_path,
        &csv_report_path,
        &zip_path,
    )?;

    tx.send(ProgressEvent::stage_complete(
        PipelineStage::Reports,
        "Reports generated",
        stage_start.elapsed(),
    ))
    .ok();

    // ── Complete ──────────────────────────────────────────────────────────
    let pipeline_elapsed = pipeline_start.elapsed();
    let total_dropped = summary.dropped_missing.len()
        + summary.dropped_gini.len()
        + summary.dropped_correlation.len();

    // Split into message + detail so the path doesn't get truncated
    // in the 66-wide progress overlay box.
    tx.send(ProgressEvent {
        stage: PipelineStage::Complete,
        message: format!("Done: {} features dropped", total_dropped),
        detail: Some(format!("Output: {}", output_path.display())),
        is_complete: true,
        elapsed_secs: Some(pipeline_elapsed.as_secs_f64()),
        summary: Some(crate::pipeline::progress::SummaryData {
            initial_features: summary.initial_features,
            final_features: summary.final_features,
            dropped_missing: summary.dropped_missing.len(),
            dropped_gini: summary.dropped_gini.len(),
            dropped_correlation: summary.dropped_correlation.len(),
        }),
    })
    .ok();

    Ok(())
}

// ============================================================================
// Pipeline execution: terminal / indicatif path (--no-confirm)
// ============================================================================

fn run_pipeline_no_tui(mut config: PipelineConfig) -> Result<()> {
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

    // Validate target and setup weights (returns None if user cancelled)
    let Some(weights) = validate_target_and_weights(&df, &mut config, true)? else {
        return Ok(());
    };

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

    // Build metadata maps for IV-first correlation drop logic
    let (feature_metadata, feature_types) =
        build_correlation_metadata(&gini_analyses, &missing_ratios);

    // Run correlation analysis
    let (correlated_pairs, features_to_drop_corr) = run_correlation_analysis(
        &mut df,
        &config,
        &weights,
        &mut summary,
        &feature_metadata,
        &feature_types,
    )?;
    report_builder.set_correlation_results(&correlated_pairs, &features_to_drop_corr);

    // Save results
    save_results(&mut df, &output_path, &mut summary)?;

    // Build and export reduction report
    report_builder.set_timing(&summary);
    let report = report_builder.build();
    let report_path = derive_output_path(&input, "reduction_report", "json");
    export_reduction_report(&report, &report_path)?;

    // Also export CSV summary for easy viewing
    let csv_report_path = derive_output_path(&input, "reduction_report", "csv");
    export_reduction_report_csv(&report, &csv_report_path)?;

    // Package all three reports into a zip file
    let gini_analysis_path = derive_output_path(&input, "gini_analysis", "json");
    let zip_path = derive_output_path(&input, "reduction_report", "zip");
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

    Ok(())
}

// ============================================================================
// Shared stage helpers (used by both paths)
// ============================================================================

/// Load dataset and apply initial column drops (indicatif terminal path)
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
    let dropped_count = apply_initial_drops(&mut df, columns_to_drop);
    if dropped_count > 0 {
        print_success(&format!(
            "Dropped {} user-specified column(s)",
            dropped_count
        ));
    }

    let initial_features = cols - dropped_count;
    let mut summary = ReductionSummary::new(initial_features);
    let load_elapsed = step_start.elapsed();
    summary.set_load_time(load_elapsed);
    print_step_time(load_elapsed);

    Ok((df, initial_features, summary))
}

/// Load dataset and apply initial column drops (TUI / channel path)
fn load_and_prepare_dataset_with_tx(
    input: &std::path::Path,
    columns_to_drop: &[String],
    infer_schema_length: usize,
    tx: &ProgressSender,
) -> Result<(polars::prelude::DataFrame, usize, ReductionSummary)> {
    let step_start = Instant::now();
    let (mut df, _rows, cols, _memory_mb) =
        load_dataset_with_progress_channel(input, infer_schema_length, tx)?;

    // Apply user-specified column drops
    let dropped_count = apply_initial_drops(&mut df, columns_to_drop);

    let initial_features = cols - dropped_count;
    let mut summary = ReductionSummary::new(initial_features);
    let load_elapsed = step_start.elapsed();
    summary.set_load_time(load_elapsed);

    Ok((df, initial_features, summary))
}

fn apply_initial_drops(df: &mut polars::prelude::DataFrame, columns_to_drop: &[String]) -> usize {
    if columns_to_drop.is_empty() {
        return 0;
    }
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
        let taken = std::mem::take(df);
        *df = taken.drop_many(&valid_columns);
    }
    count
}

/// Validate target column (headless version for TUI path — does NOT show interactive prompts).
/// Returns the weights vector or an error.
fn validate_target_and_weights_headless(
    df: &polars::prelude::DataFrame,
    config: &mut PipelineConfig,
) -> Result<Vec<f64>> {
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

    let weights = get_weights(df, config.weight_column.as_deref())?;

    // If target_mapping was already supplied (by wizard), skip binary check.
    if config.target_mapping.is_none() {
        match analyze_target_column(df, &config.target)? {
            TargetAnalysis::AlreadyBinary => {}
            TargetAnalysis::NeedsMapping { unique_values } => {
                anyhow::bail!(
                    "Target column '{}' is not binary (0/1). Found {} unique values: {:?}\n\
                     Please provide target mapping via the wizard or --event-value/--non-event-value.",
                    config.target,
                    unique_values.len(),
                    &unique_values[..unique_values.len().min(5)]
                );
            }
        }
    }

    Ok(weights)
}

/// Validate target column and setup sample weights.
/// Returns `Ok(None)` if the user cancelled the target mapping selection.
fn validate_target_and_weights(
    df: &polars::prelude::DataFrame,
    config: &mut PipelineConfig,
    no_confirm: bool,
) -> Result<Option<Vec<f64>>> {
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
    if let Some(weight_col) = &config.weight_column {
        print_success(&format!("Using weight column: '{}'", weight_col));
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
                        return Ok(None);
                    }
                }
            }
        }
    } else if let Some(mapping) = &config.target_mapping {
        // Mapping was provided via CLI - display it
        println!(
            "   {} Using target mapping: '{}' → 1, '{}' → 0",
            style("✓").green(),
            mapping.event_value,
            mapping.non_event_value
        );
    }

    Ok(Some(weights))
}

/// Run missing value analysis (indicatif path)
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

        let taken = std::mem::take(df);
        *df = taken.drop_many(&features_to_drop_missing);
        summary.add_missing_drops(features_to_drop_missing.clone());
        print_success("Dropped features with high missing values");
    }

    let missing_elapsed = step_start.elapsed();
    summary.set_missing_time(missing_elapsed);
    print_step_time(missing_elapsed);

    Ok((missing_ratios, features_to_drop_missing))
}

/// Run missing value analysis (background / channel path)
#[allow(clippy::type_complexity)]
fn run_missing_analysis_bg(
    df: &mut polars::prelude::DataFrame,
    config: &PipelineConfig,
    weights: &[f64],
    summary: &mut ReductionSummary,
) -> Result<(Vec<(String, f64)>, Vec<String>)> {
    let step_start = Instant::now();
    let missing_ratios = analyze_missing_values(df, weights, config.weight_column.as_deref())?;
    let features_to_drop_missing =
        get_features_above_threshold(&missing_ratios, config.missing_threshold, &config.target);

    if !features_to_drop_missing.is_empty() {
        let taken = std::mem::take(df);
        *df = taken.drop_many(&features_to_drop_missing);
        summary.add_missing_drops(features_to_drop_missing.clone());
    }

    let missing_elapsed = step_start.elapsed();
    summary.set_missing_time(missing_elapsed);

    Ok((missing_ratios, features_to_drop_missing))
}

/// Run Gini/IV analysis (indicatif path)
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
    let solver_config = build_solver_config(config)?;

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

    export_gini(
        &gini_analyses,
        &features_to_drop_gini,
        config,
        input,
        binning_strategy,
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

/// Run Gini/IV analysis (background / channel path)
fn run_gini_analysis_bg(
    df: &polars::prelude::DataFrame,
    config: &PipelineConfig,
    input: &std::path::Path,
    weights: &[f64],
    summary: &mut ReductionSummary,
    tx: &ProgressSender,
) -> Result<(Vec<pipeline::IvAnalysis>, Vec<String>)> {
    let binning_strategy: BinningStrategy = config
        .binning_strategy
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let solver_config = build_solver_config(config)?;

    let step_start = Instant::now();
    let gini_analyses = analyze_features_iv_with_progress(
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
        tx,
    )?;
    let features_to_drop_gini = get_low_gini_features(&gini_analyses, config.gini_threshold);

    export_gini(
        &gini_analyses,
        &features_to_drop_gini,
        config,
        input,
        binning_strategy,
    )?;

    if !features_to_drop_gini.is_empty() {
        summary.add_gini_drops(features_to_drop_gini.clone());
    }

    let gini_elapsed = step_start.elapsed();
    summary.set_gini_time(gini_elapsed);

    Ok((gini_analyses, features_to_drop_gini))
}

/// Build `FeatureMetadata` and `FeatureType` maps from the Gini/IV and missing
/// analysis stages.  These are consumed by the correlation drop logic.
fn build_correlation_metadata(
    gini_analyses: &[pipeline::IvAnalysis],
    missing_ratios: &[(String, f64)],
) -> (
    std::collections::HashMap<String, FeatureMetadata>,
    std::collections::HashMap<String, pipeline::FeatureType>,
) {
    let missing_lookup: std::collections::HashMap<&str, f64> = missing_ratios
        .iter()
        .map(|(n, r)| (n.as_str(), *r))
        .collect();

    // Single pass over gini_analyses to build both maps (avoids double iteration
    // and halves String clone count).
    let mut feature_metadata = std::collections::HashMap::with_capacity(gini_analyses.len());
    let mut feature_types = std::collections::HashMap::with_capacity(gini_analyses.len());

    for a in gini_analyses {
        feature_types.insert(a.feature_name.clone(), a.feature_type);
        feature_metadata.insert(
            a.feature_name.clone(),
            FeatureMetadata {
                iv: Some(a.iv),
                missing_ratio: missing_lookup.get(a.feature_name.as_str()).copied(),
            },
        );
    }

    (feature_metadata, feature_types)
}

/// Run correlation analysis (indicatif path)
fn run_correlation_analysis(
    df: &mut polars::prelude::DataFrame,
    config: &PipelineConfig,
    weights: &[f64],
    summary: &mut ReductionSummary,
    feature_metadata: &std::collections::HashMap<String, FeatureMetadata>,
    feature_types: &std::collections::HashMap<String, pipeline::FeatureType>,
) -> Result<(Vec<pipeline::CorrelatedPair>, Vec<FeatureToDrop>)> {
    print_step_header(3, "Correlation Analysis");

    let step_start = Instant::now();
    let correlated_pairs = find_correlated_pairs_auto(
        df,
        config.correlation_threshold,
        weights,
        config.weight_column.as_deref(),
        Some(feature_types),
    )?;
    let features_to_drop_corr =
        select_features_to_drop(&correlated_pairs, &config.target, Some(feature_metadata));
    print_success("Correlation analysis complete");

    apply_correlation_drops(df, &correlated_pairs, &features_to_drop_corr, summary);

    let correlation_elapsed = step_start.elapsed();
    summary.set_correlation_time(correlation_elapsed);
    print_step_time(correlation_elapsed);

    Ok((correlated_pairs, features_to_drop_corr))
}

/// Run correlation analysis (background / channel path)
fn run_correlation_analysis_bg(
    df: &mut polars::prelude::DataFrame,
    config: &PipelineConfig,
    weights: &[f64],
    summary: &mut ReductionSummary,
    tx: &ProgressSender,
    feature_metadata: &std::collections::HashMap<String, FeatureMetadata>,
    feature_types: &std::collections::HashMap<String, pipeline::FeatureType>,
) -> Result<(Vec<pipeline::CorrelatedPair>, Vec<FeatureToDrop>)> {
    let step_start = Instant::now();
    let correlated_pairs = find_correlated_pairs_auto_with_progress(
        df,
        config.correlation_threshold,
        weights,
        config.weight_column.as_deref(),
        Some(feature_types),
        tx,
    )?;
    let features_to_drop_corr =
        select_features_to_drop(&correlated_pairs, &config.target, Some(feature_metadata));

    apply_correlation_drops(df, &correlated_pairs, &features_to_drop_corr, summary);

    let correlation_elapsed = step_start.elapsed();
    summary.set_correlation_time(correlation_elapsed);

    Ok((correlated_pairs, features_to_drop_corr))
}

fn apply_correlation_drops(
    df: &mut polars::prelude::DataFrame,
    correlated_pairs: &[pipeline::CorrelatedPair],
    features_to_drop_corr: &[FeatureToDrop],
    summary: &mut ReductionSummary,
) {
    if correlated_pairs.is_empty() {
        return;
    }
    if !features_to_drop_corr.is_empty() {
        let drop_names: Vec<String> = features_to_drop_corr
            .iter()
            .map(|f| f.feature.clone())
            .collect();
        let taken = std::mem::take(df);
        *df = taken.drop_many(&drop_names);
        summary.add_correlation_drops(drop_names);
    }
}

/// Save results to output file (indicatif path)
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

/// Save results to output file (background path)
fn save_results_bg(
    df: &mut polars::prelude::DataFrame,
    output_path: &std::path::Path,
    summary: &mut ReductionSummary,
) -> Result<()> {
    let step_start = Instant::now();
    save_dataset(df, output_path)?;
    let save_elapsed = step_start.elapsed();
    summary.set_save_time(save_elapsed);
    Ok(())
}

// ============================================================================
// Shared pure helpers
// ============================================================================

fn build_solver_config(config: &PipelineConfig) -> Result<Option<SolverConfig>> {
    if config.use_solver {
        let monotonicity: MonotonicityConstraint = config
            .monotonicity
            .parse()
            .map_err(|e: String| anyhow::anyhow!(e))?;
        Ok(Some(SolverConfig {
            timeout_seconds: config.solver_timeout,
            gap_tolerance: config.solver_gap,
            monotonicity,
            min_bin_samples: 5,
        }))
    } else {
        Ok(None)
    }
}

fn export_gini(
    gini_analyses: &[pipeline::IvAnalysis],
    features_to_drop_gini: &[String],
    config: &PipelineConfig,
    input: &std::path::Path,
    binning_strategy: BinningStrategy,
) -> Result<()> {
    let gini_output_path = derive_output_path(input, "gini_analysis", "json");
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
        gini_analyses,
        features_to_drop_gini,
        &gini_output_path,
        &export_params,
    )
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
