# Architecture

## Overview

Lo-phi is a Rust CLI tool designed to reduce feature sets in tabular datasets through automated statistical analysis. The tool performs three sequential reduction strategies: **missing value analysis** (drops features exceeding a null ratio threshold), **univariate Gini/IV analysis** (drops features with low predictive power using Weight of Evidence binning), and **correlation analysis** (drops one feature from highly correlated pairs). This sequential pipeline approach ensures that users can systematically eliminate redundant, irrelevant, or problematic features before model development.

The architecture prioritizes memory efficiency, parallel processing, and user experience. It leverages Polars for lazy/streaming DataFrame operations, Rayon for CPU-bound parallelization, and Ratatui for an interactive terminal UI. All statistical computations use pure-Rust implementations to eliminate external dependencies (faer for matrix operations, HiGHS for optimization). The tool outputs both the reduced dataset and comprehensive reports in JSON, CSV, and ZIP formats.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Lo-phi CLI Tool                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                  ┌───────────────────────┐
                  │   CLI Module          │
                  │ ─────────────────────  │
                  │ • args.rs             │
                  │ • config_menu.rs      │
                  │ • convert.rs          │
                  └───────────┬───────────┘
                              │
                              ▼
                  ┌───────────────────────┐
                  │   Pipeline Module     │
                  │ ─────────────────────  │
                  │ • loader.rs           │
                  │ • missing.rs          │
                  │ • iv.rs               │
                  │ • correlation.rs      │
                  │ • target.rs           │
                  │ • weights.rs          │
                  │ • solver.rs           │
                  └───────────┬───────────┘
                              │
                              ▼
                  ┌───────────────────────┐
                  │   Report Module       │
                  │ ─────────────────────  │
                  │ • summary.rs          │
                  │ • gini_export.rs      │
                  │ • reduction_report.rs │
                  └───────────┬───────────┘
                              │
                              ▼
                  ┌───────────────────────┐
                  │   Utils Module        │
                  │ ─────────────────────  │
                  │ • progress.rs         │
                  │ • styling.rs          │
                  └───────────────────────┘
```

## Module Structure

### CLI Module (`src/cli/`)

The CLI module handles all user interaction and configuration management. It consists of three submodules:

- **`args.rs`**: Defines the command-line argument structure using `clap::Parser`. Supports both direct CLI arguments (`--target`, `--missing-threshold`, etc.) and the `--no-confirm` flag to bypass interactive prompts. Also defines the `Commands::Convert` subcommand for CSV-to-Parquet conversion.

- **`config_menu.rs`**: Implements the interactive TUI configuration menu using Ratatui and Crossterm. Provides a three-column layout with keyboard shortcuts (`[T]` for target selection, `[C]` for threshold editing, `[S]` for solver configuration, `[D]` for drop columns, `[W]` for weights, `[F]` for conversion, `[Enter]` to run). Includes file selector for browsing datasets and target mapping selector for non-binary target columns. Returns `ConfigResult::Proceed`, `ConfigResult::Convert`, or `ConfigResult::Quit`.

- **`convert.rs`**: Handles CSV-to-Parquet conversion with two modes: fast in-memory mode (default for TUI) and streaming mode for large files. Uses Polars lazy API to minimize memory footprint during schema inference and writing.

### Pipeline Module (`src/pipeline/`)

The pipeline module contains the core statistical analysis algorithms. Each submodule implements one stage of the reduction pipeline:

- **`loader.rs`**: Loads CSV or Parquet files using Polars with progress tracking via indicatif. Detects file format from extension, applies schema inference (configurable length), and returns row/column counts plus estimated memory usage.

- **`missing.rs`**: Calculates weighted null ratios for each column. Supports sample weights via the `--weight-column` option. Returns a vector of `(feature_name, missing_ratio)` tuples sorted by ratio descending.

- **`iv.rs`**: The most complex module (~2600 lines). Performs Weight of Evidence (WoE) binning and Information Value (IV) / Gini coefficient calculation. Supports two binning strategies: `BinningStrategy::Quantile` (equal-frequency) and `BinningStrategy::Cart` (decision-tree splits). Optionally uses HiGHS solver for monotonic binning constraints (ascending, descending, peak, valley, auto-detection). Handles both numeric and categorical features with separate binning logic. Returns `IvAnalysis` structs containing bins, WoE values, event/non-event distributions, IV, and Gini.

- **`correlation.rs`**: Computes pairwise Pearson correlation using Welford's algorithm for numerical stability. Excludes categorical features and the target column. Uses faer for matrix-based computation. Returns `CorrelatedPair` structs with feature names and correlation coefficients. Implements `select_features_to_drop()` to choose which feature to drop from each pair (preserves target if involved).

- **`target.rs`**: Analyzes the target column to determine if binary mapping is required. Returns `TargetAnalysis::AlreadyBinary` for 0/1 columns or `TargetAnalysis::NeedsMapping` with unique values for non-binary targets. Supports `TargetMapping` to convert arbitrary values (e.g., "Yes"/"No") to 0/1 encoding.

- **`weights.rs`**: Extracts sample weights from a specified column. Validates non-negative weights and returns a `Vec<f64>` matching DataFrame row count. Defaults to uniform weights (1.0) if no weight column is specified.

- **`solver.rs`**: Configures and invokes the HiGHS mixed-integer programming solver for optimal monotonic binning. Defines `MonotonicityConstraint` (none, ascending, descending, peak, valley, auto) and `SolverConfig` (timeout, gap tolerance, minimum bin samples). See [algorithms.md](algorithms.md) for constraint formulation details.

### Report Module (`src/report/`)

The report module generates output files summarizing the reduction process:

- **`summary.rs`**: Defines `ReductionSummary` struct to track dropped features and execution times for each pipeline stage (load, missing, Gini, correlation, save). Implements `display()` method to print a terminal summary table showing initial/final feature counts and total reduction percentage.

- **`gini_export.rs`**: Exports detailed Gini/IV analysis to `{input}_gini_analysis.json`. Includes per-feature binning results with WoE values, event/non-event distributions, bin ranges/categories, IV contribution, and Gini coefficient. Used for model interpretation and manual binning review.

- **`reduction_report.rs`**: Builds comprehensive reduction reports in two formats:
  - **JSON** (`{input}_reduction_report.json`): Structured data including all configuration parameters, dropped features per stage, timing breakdowns, and full correlation matrices.
  - **CSV** (`{input}_reduction_report.csv`): Human-readable summary with one row per feature. Includes feature name, missing ratio, Gini score, and pipe-separated list of correlated features (e.g., `feature2: 0.92 | feature3: 0.88`).

  Also packages the Gini JSON, report JSON, and report CSV into a single ZIP archive (`{input}_reduction_report.zip`) via `package_reduction_reports()`.

### Utils Module (`src/utils/`)

Provides progress tracking and terminal styling utilities:

- **`progress.rs`**: Wraps `indicatif::ProgressBar` with helper functions `create_spinner()`, `finish_with_success()`, and spinner styles for long-running operations (loading, analyzing).

- **`styling.rs`**: Defines terminal output formatting functions using `console` crate for colored, styled output. Includes `print_banner()`, `print_step_header()`, `print_success()`, `print_count()`, `print_config()`, and `print_completion()`. Ensures consistent styling across all CLI output.

## Pipeline Flow

The reduction pipeline executes sequentially through five stages, orchestrated by `src/main.rs`:

### Stage 0: Configuration

1. **Argument Parsing**: Clap parses CLI arguments. If no input file is provided, launches `run_file_selector()` TUI to browse for datasets.
2. **Interactive Configuration**: If `--no-confirm` is not set, displays `run_config_menu()` with current settings. User can edit thresholds, select target/weight columns, configure solver options, or convert CSV to Parquet.
3. **Target Validation**: Analyzes target column with `analyze_target_column()`. If not binary (0/1), prompts `run_target_mapping_selector()` to map unique values to event/non-event labels.

### Stage 1: Load Dataset

1. **Load with Progress**: `load_dataset_with_progress()` reads CSV/Parquet using Polars. Displays progress bar during schema inference and parsing.
2. **Initial Drops**: Applies user-specified `--drop-columns` to remove features before analysis.
3. **Weight Extraction**: Calls `get_weights()` to extract sample weights if `--weight-column` is specified. Validates non-negative weights.

**Data Transformation**: Raw CSV/Parquet → Polars DataFrame with optional target mapping and weight extraction.

### Stage 2: Missing Value Analysis

1. **Calculate Ratios**: `analyze_missing_values()` computes weighted null ratio per column using Rayon for parallelization.
2. **Identify Drops**: `get_features_above_threshold()` filters features exceeding `--missing-threshold` (default 0.30).
3. **Apply Drops**: Removes identified features from DataFrame using `df.drop_many()`.

**Data Transformation**: DataFrame with all features → DataFrame excluding high-missing features. Dropped features tracked in `ReductionSummary`.

### Stage 3: Gini/IV Analysis

1. **Binning**: `analyze_features_iv()` bins each numeric/categorical feature using specified strategy (CART or Quantile). For numeric features with solver enabled, applies monotonicity constraints via HiGHS optimization.
2. **WoE Calculation**: Computes Weight of Evidence per bin: `WoE = ln((event_rate / (1 - event_rate)) / (population_event_rate / (1 - population_event_rate)))`.
3. **IV/Gini Aggregation**: Sums IV contributions across bins. Calculates Gini coefficient from cumulative gain curves.
4. **Identify Drops**: `get_low_gini_features()` filters features below `--gini-threshold` (default 0.05).
5. **Export Analysis**: Saves detailed binning results to `{input}_gini_analysis.json` via `export_gini_analysis_enhanced()`.
6. **Apply Drops**: Removes low-Gini features from DataFrame.

**Data Transformation**: DataFrame without high-missing features → DataFrame excluding low-predictive-power features. WoE bins and Gini scores stored in `IvAnalysis` structs.

### Stage 4: Correlation Analysis

1. **Compute Correlations**: `find_correlated_pairs_auto()` calculates Pearson correlation for all numeric feature pairs using weighted means and Welford's algorithm. Uses faer matrix operations for efficiency.
2. **Identify Pairs**: Filters pairs exceeding `--correlation-threshold` (default 0.40).
3. **Select Drops**: `select_features_to_drop()` chooses one feature from each correlated pair. Preserves the target column if involved in a pair.
4. **Apply Drops**: Removes selected features from DataFrame.

**Data Transformation**: DataFrame without low-Gini features → Final reduced DataFrame with decorrelated features. Correlated pairs stored in `CorrelatedPair` structs.

### Stage 5: Save and Report

1. **Save Dataset**: `save_dataset()` writes reduced DataFrame to `{output}` (CSV or Parquet based on extension).
2. **Generate Reports**:
   - Builds comprehensive `ReductionReport` via `ReductionReportBuilder`.
   - Exports JSON report, CSV summary, and Gini analysis.
   - Packages all three into `{input}_reduction_report.zip`.
3. **Display Summary**: `summary.display()` prints terminal table with final statistics and execution times.

**Data Transformation**: Reduced DataFrame → Persisted file + bundled ZIP reports. All dropped features and analysis metadata preserved for auditing.

### State Management

- **PipelineConfig**: Immutable struct holding all configuration parameters (thresholds, binning strategy, solver options, column names). Constructed once during setup.
- **ReductionSummary**: Mutable accumulator tracking dropped features and timing per stage. Updated after each pipeline step.
- **DataFrame Mutations**: Each analysis stage returns a list of features to drop. The main pipeline applies drops sequentially using `df.drop_many()`, ensuring pipeline stages operate independently.

## Key Design Patterns

### Error Handling

Lo-phi uses a two-tier error handling strategy following Rust best practices:

- **Domain Errors**: Module-specific error types defined with `thiserror` for structured error handling. Examples: `TargetNotFoundError`, `BinningError`, `SolverTimeoutError`. These errors carry context (feature name, threshold value, etc.) for debugging.
- **Handler Errors**: Top-level functions in `main.rs` and CLI handlers use `anyhow::Result` for ergonomic error propagation. Context is added via `.with_context()` to provide user-friendly error messages (e.g., "Failed to load dataset from path: ...").

This pattern ensures library code (`src/pipeline/`, `src/report/`) remains reusable with precise error types while CLI code remains concise with error chain reporting.

### Progress Reporting

All long-running operations display progress indicators using `indicatif`:

- **Spinners**: Used for indeterminate operations (missing analysis, correlation computation, saving files). Created via `create_spinner()` with styled messages and finished with `finish_with_success()`.
- **Progress Bars**: Used for deterministic operations (dataset loading with row counts). Shows percentage, ETA, and throughput (rows/second).
- **Step Headers**: Each pipeline stage prints a numbered header (`print_step_header()`) followed by timing (`print_step_time()`). Creates visual separation and progress tracking.

### Parallel Processing

CPU-bound operations leverage Rayon for data parallelism:

- **Missing Analysis**: `par_iter()` over DataFrame columns to compute null ratios in parallel.
- **IV Analysis**: `par_iter()` over feature list to bin and calculate WoE independently. Each feature's analysis is embarrassingly parallel.
- **Correlation**: Uses faer's matrix multiplication which internally parallelizes BLAS-like operations.

This approach maximizes CPU utilization on multi-core systems without manual thread management. No shared mutable state is required due to functional map/reduce patterns.

### Memory Efficiency

Polars' lazy evaluation and streaming capabilities minimize memory footprint:

- **Lazy Loading**: CSV/Parquet files are read with schema inference on a limited sample (`--infer-schema-length`, default 10,000 rows) to avoid full scans.
- **Streaming Writes**: Output files are written in chunks to avoid loading the entire reduced DataFrame into memory.
- **Column Dropping**: Features are dropped immediately after each stage to release memory before the next analysis.

See [ADR-001](adr/ADR-001-polars-framework.md) for the rationale behind choosing Polars over alternatives like ndarray or Apache Arrow.

## Technology Stack

### Core Dependencies

- **Polars 0.46** ([polars-rs/polars](https://github.com/pola-rs/polars)): DataFrame library with lazy evaluation, CSV/Parquet support, streaming, and full dtype support. Chosen for memory efficiency and performance on large datasets.
- **Rayon 1.10** ([rayon-rs/rayon](https://github.com/rayon-rs/rayon)): Data parallelism library for CPU-bound tasks. Provides `par_iter()` for parallel iterators with work-stealing scheduling.
- **Ratatui 0.29** ([ratatui-org/ratatui](https://github.com/ratatui-org/ratatui)): Terminal UI framework for interactive configuration menu. Replaces legacy tui-rs with active maintenance and Crossterm backend. See [ADR-008](adr/ADR-008-ratatui-tui.md).
- **Crossterm 0.28** ([crossterm-rs/crossterm](https://github.com/crossterm-rs/crossterm)): Cross-platform terminal manipulation (keyboard input, cursor control, colors). Backend for Ratatui.
- **HiGHS Solver (via good_lp 1.8)** ([rust-or/good_lp](https://github.com/rust-or/good_lp)): Mixed-integer programming solver for optimal monotonic binning. Uses pure-Rust HiGHS backend (no external libraries required). Supports timeout and MIP gap tolerance.
- **faer 0.20** ([sarah-quinones/faer-rs](https://github.com/sarah-quinones/faer-rs)): Pure-Rust linear algebra library for matrix-based correlation computation. Replaces ndarray-linalg to eliminate BLAS/LAPACK dependencies.

### Supporting Dependencies

- **Clap 4.5** ([clap-rs/clap](https://github.com/clap-rs/clap)): Command-line argument parsing with derive macros.
- **Indicatif 0.17** ([console-rs/indicatif](https://github.com/console-rs/indicatif)): Progress bars and spinners with styled output.
- **Anyhow 1.0** ([dtolnay/anyhow](https://github.com/dtolnay/anyhow)): Ergonomic error handling for application code.
- **Thiserror 2.0** ([dtolnay/thiserror](https://github.com/dtolnay/thiserror)): Derive macros for custom error types in library code.
- **Serde 1.0 + Serde JSON 1.0** ([serde-rs/serde](https://github.com/serde-rs/serde)): JSON serialization for Gini analysis and reduction reports.
- **Zip 2.2** ([zip-rs/zip2](https://github.com/zip-rs/zip2)): ZIP archive creation for bundled reports. Uses deflate compression (pure Rust, no external dependencies).

See [developer-guide.md](developer-guide.md) for build instructions and testing strategies. Refer to [glossary.md](glossary.md) for definitions of statistical terms (WoE, IV, Gini, Pearson correlation).
