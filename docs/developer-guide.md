# Developer Guide

This guide provides comprehensive onboarding for contributors to the Lo-phi project. It covers development setup, testing, code conventions, and common development workflows.

## Prerequisites

Before contributing to Lo-phi, ensure you have the following installed:

- **Rust toolchain** (stable channel, 1.70 or later) - Install from [rustup.rs](https://rustup.rs/)
- **Git** (2.30 or later)
- **Python** (3.8+, optional) - Required only for test data generation scripts

Lo-phi is a pure-Rust project with no external system dependencies. All mathematical operations use native Rust libraries (faer for linear algebra, HiGHS for optimization via good_lp). The project targets Linux, macOS, and Windows.

### Platform Support

- **Linux**: Fully supported (primary development platform)
- **macOS**: Fully supported (Apple Silicon and Intel)
- **Windows**: Fully supported (x86_64)

## Getting Started

### Clone the Repository

```bash
git clone https://github.com/yourusername/lo-phi.git
cd lo-phi
```

### Build Debug Binary

The debug build includes additional runtime checks and is faster to compile:

```bash
cargo build
```

The resulting binary will be at `target/debug/lophi`.

### Build Release Binary

The release build is optimized for performance and suitable for benchmarking:

```bash
cargo build --release
```

The resulting binary will be at `target/release/lophi`. Release builds are significantly faster for large datasets due to LLVM optimizations.

## Project Structure

Lo-phi follows a modular architecture organized into five primary modules. For detailed module responsibilities and data flow, see [architecture.md](architecture.md).

```
src/
├── cli/              # Command-line interface
│   ├── args.rs       # Clap argument definitions
│   ├── config_menu.rs # Interactive TUI configuration menu (Ratatui)
│   └── convert.rs    # CSV-to-Parquet conversion subcommand
├── pipeline/         # Core analysis algorithms
│   ├── loader.rs     # Dataset loading with progress tracking
│   ├── missing.rs    # Null ratio calculation
│   ├── iv.rs         # WoE/IV/Gini binning analysis (2600+ lines)
│   ├── correlation.rs # Pearson correlation with Welford algorithm
│   ├── target.rs     # Binary/non-binary target handling
│   ├── weights.rs    # Sample weight validation and extraction
│   └── solver/       # MIP solver for optimal binning
├── report/           # Output generation
│   ├── summary.rs    # Terminal summary tables
│   ├── gini_export.rs # JSON export of Gini/IV analysis
│   └── reduction_report.rs # Comprehensive reduction report
└── utils/            # Terminal UI utilities
    ├── progress.rs   # Progress bars and spinners
    └── styling.rs    # Terminal colors and formatting
```

The pipeline flow is sequential: **Configuration → Load → Missing Analysis → Gini/IV Analysis → Correlation Analysis → Save & Report**. Each stage drops features based on its criteria, and the reduced dataset flows to the next stage.

## Building

### Standard Builds

```bash
# Debug build (faster compilation, runtime checks enabled)
cargo build

# Release build (optimized, suitable for production use)
cargo build --release
```

### Feature Flags

Lo-phi uses the `--all-features` flag in testing to ensure all Polars features are available. The key Polars features enabled are:

- `lazy` - Lazy DataFrame evaluation for memory efficiency
- `csv` - CSV file reading/writing
- `parquet` - Parquet file reading/writing
- `dtype-full` - Full data type support
- `streaming` - Streaming query execution

These features are defined in `Cargo.toml` and are always enabled (not conditional). The `--all-features` flag is used primarily for consistency with other crates that may have optional features.

## Testing

Lo-phi has a comprehensive test suite with 163 total tests: **65 unit tests** (in `src/` modules) and **98 integration tests** (in `tests/` directory).

### Run All Tests

```bash
cargo test --all-features
```

This runs both unit tests and integration tests with all Polars features enabled.

### Run Unit Tests Only

Unit tests are embedded in source files using `#[test]` attributes. They test individual functions and modules in isolation:

```bash
cargo test --lib --all-features
```

Unit tests exist in:
- `src/pipeline/target.rs` - Target column validation
- `src/pipeline/weights.rs` - Weight column validation
- `src/pipeline/iv.rs` - WoE binning logic
- `src/pipeline/solver/` - MIP solver components
- `src/report/reduction_report.rs` - Report generation

### Run Integration Tests Only

Integration tests are in the `tests/` directory and test end-to-end workflows:

```bash
cargo test --test '*' --all-features
```

Integration test files (8 total):
- `test_pipeline.rs` - Full pipeline execution
- `test_missing.rs` - Missing value analysis
- `test_correlation.rs` - Correlation analysis
- `test_target_mapping.rs` - Non-binary target mapping
- `test_solver.rs` - Optimal binning solver
- `test_loader.rs` - Dataset loading
- `test_convert.rs` - CSV-to-Parquet conversion
- `test_cli.rs` - CLI argument parsing

### Run Specific Test

To run a single test by name with output visible:

```bash
cargo test --all-features test_name -- --nocapture
```

Or using the Makefile shortcut:

```bash
make test-one TEST=test_name
```

### Run Tests With Output Visible

By default, Rust captures stdout/stderr during tests. To see print statements and progress bars:

```bash
cargo test --all-features -- --nocapture
```

Or via Makefile:

```bash
make test-verbose
```

### Full CI Check

Run the complete CI pipeline locally (formatting + linting + tests):

```bash
make check
```

This executes:
1. `cargo fmt -- --check` - Verify code formatting
2. `cargo clippy --all-targets --all-features -- -D warnings` - Lint with Clippy (treat warnings as errors)
3. `cargo test --all-features` - Run all tests

This is the same check that runs in GitHub Actions CI.

### Test Structure and Fixtures

All integration tests share common utilities in `tests/common/mod.rs`:

**Fixture Generators:**
- `create_test_dataframe()` - Standard test DataFrame with known patterns (10 rows, 6 columns including target)
- `create_large_test_dataframe(rows, cols)` - Random DataFrame for performance tests
- `create_missing_test_dataframe()` - DataFrame with specific missing value ratios
- `create_correlation_test_dataframe()` - DataFrame with known correlation patterns (perfect, negative, uncorrelated)
- `create_binary_target_dataframe()` - 50/50 target split for IV/Gini tests

**File Helpers:**
- `create_temp_csv(df)` - Write DataFrame to temporary CSV file
- `create_temp_parquet(df)` - Write DataFrame to temporary Parquet file

**Assertion Helpers:**
- `assert_shape(df, rows, cols)` - Verify DataFrame dimensions
- `assert_has_columns(df, cols)` - Verify expected columns exist
- `assert_missing_columns(df, cols)` - Verify columns were dropped

Example usage:

```rust
use common::*;

#[test]
fn test_example() {
    let df = create_test_dataframe();
    let (temp_dir, path) = create_temp_parquet(&mut df);

    // Run pipeline...

    assert_shape(&result, 10, 4);
    assert_has_columns(&result, &["target", "feature_good"]);
}
```

## Benchmarks

Lo-phi uses Criterion for benchmarking. Two benchmark suites are available:

### Binning Benchmark

Compares CART vs Quantile binning strategies across varying dataset sizes:

```bash
cargo bench --bench binning_benchmark
```

This benchmark is located in `benches/binning_benchmark.rs` and measures:
- Binning time vs number of features
- Binning time vs number of rows
- CART vs Quantile strategy performance

### Correlation Benchmark

Measures correlation analysis performance for different dataset dimensions:

```bash
cargo bench --bench correlation_benchmark
```

This benchmark is located in `benches/correlation_benchmark.rs` and measures:
- Correlation computation time vs feature count
- Memory usage patterns
- Welford algorithm efficiency

### Run All Benchmarks

```bash
cargo bench
```

Criterion generates HTML reports in `target/criterion/` with performance graphs and statistical analysis.

## CI/CD

Lo-phi uses GitHub Actions for continuous integration and release builds.

### CI Workflow

File: `.github/workflows/ci.yml`

**Trigger**: Pull requests to `main` branch, manual dispatch

**Jobs:**
1. **Test Suite** (Ubuntu):
   - Runs all tests (`cargo test --verbose --all-features`)
   - Runs Clippy linter (`cargo clippy --all-targets --all-features -- -D warnings`)
   - Checks formatting (`cargo fmt -- --check`)
   - Uses Rust stable toolchain with `clippy` and `rustfmt` components
   - Caches cargo registry for faster builds

2. **Build** (macOS and Windows):
   - Builds release binaries on macOS-latest and Windows-latest
   - Uploads binaries as artifacts (`lophi-macos-latest`, `lophi-windows-latest`)
   - Verifies cross-platform compilation without running tests

### Release Workflow

File: `.github/workflows/release.yml`

**Trigger**: Manual dispatch (workflow_dispatch)

**Jobs:**
1. **Build**: Creates optimized release binaries for:
   - Windows x86_64 (`x86_64-pc-windows-msvc`)
   - macOS Apple Silicon (`aarch64-apple-darwin`, runs on `macos-14`)

2. **Release**: Creates GitHub release with artifacts (only runs when a tag is pushed)

### Running CI Checks Locally

Before pushing code, verify all CI checks pass:

```bash
make check
```

This runs the exact same checks as GitHub Actions (formatting, linting, tests).

## Code Conventions

### Formatting

Lo-phi uses `rustfmt` with default settings. Format your code before committing:

```bash
cargo fmt
```

To check formatting without modifying files:

```bash
cargo fmt -- --check
```

### Linting

All code must pass Clippy with warnings treated as errors:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Common Clippy warnings to avoid:
- Unnecessary clones
- Redundant pattern matching
- Unused variables (prefix with `_` if intentional)
- Complex types that should use type aliases

### Error Handling

Lo-phi follows a consistent error handling pattern:

**Use `thiserror` for domain-specific errors:**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BinningError {
    #[error("Feature '{0}' has no variance")]
    NoVariance(String),

    #[error("Target column '{0}' not found")]
    TargetNotFound(String),
}
```

**Use `anyhow::Result` in main functions and handlers:**

```rust
use anyhow::{Context, Result};

pub fn load_dataset(path: &Path) -> Result<DataFrame> {
    let df = LazyFrame::scan_parquet(path, Default::default())
        .context("Failed to read Parquet file")?
        .collect()
        .context("Failed to collect DataFrame")?;
    Ok(df)
}
```

**Add context to errors:**

```rust
// Good - provides actionable context
.with_context(|| format!("Failed to analyze feature '{}'", feature_name))?

// Bad - loses context
.expect("Analysis failed")?
```

### Documentation

**Module-level docs:**

```rust
//! Module description
//!
//! Detailed explanation of module purpose and key types.
```

**Function docs:**

```rust
/// Calculate Weight of Evidence for a feature
///
/// # Arguments
///
/// * `feature` - The feature column to analyze
/// * `target` - Binary target column (0/1)
/// * `bins` - Number of bins to create
///
/// # Errors
///
/// Returns `BinningError::NoVariance` if feature has zero variance
pub fn calculate_woe(feature: &Series, target: &Series, bins: usize) -> Result<Vec<WoeBin>> {
    // ...
}
```

## Contributing

### Commit Message Format

All commit messages must start with a prefix indicating the change type:

- `fix:` - Bug fixes
- `feature:` - New features or enhancements
- `docs:` - Documentation changes
- `chore:` - Maintenance tasks (dependency updates, CI config, etc.)

Examples:

```
fix: handle zero-variance features in correlation analysis
feature: add weighted correlation support
docs: update developer guide with testing instructions
chore: update Polars to 0.46
```

**Important:** Never reference "claude" in commit messages.

### Pull Request Process

1. **Create a feature branch** from `main`:
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Make your changes** following code conventions

3. **Add tests** for new functionality:
   - Unit tests for isolated logic
   - Integration tests for end-to-end workflows

4. **Run the full check** before committing:
   ```bash
   make check
   ```

5. **Commit with proper message format**:
   ```bash
   git commit -m "feature: add support for categorical correlation"
   ```

6. **Push to your fork** and create a pull request to `main`

7. **Wait for CI to pass** - GitHub Actions will run tests on Ubuntu, macOS, and Windows

### Branch Naming

Use descriptive branch names with prefixes:

- `feature/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation improvements
- `chore/` - Maintenance tasks

Examples:
- `feature/weighted-iv-analysis`
- `fix/correlation-nan-handling`
- `docs/architecture-diagrams`

## How-To Guides

### Adding a New Pipeline Stage

To add a new analysis stage to the reduction pipeline:

1. **Create the analysis module** in `src/pipeline/`:

```rust
// src/pipeline/my_analysis.rs
use polars::prelude::*;
use anyhow::Result;

pub fn analyze_my_feature(
    df: &DataFrame,
    threshold: f64
) -> Result<Vec<String>> {
    // Return list of features to drop
    Ok(vec![])
}
```

2. **Add the module** to `src/pipeline/mod.rs`:

```rust
mod my_analysis;
pub use my_analysis::analyze_my_feature;
```

3. **Integrate into main pipeline** in `src/main.rs`:

```rust
// Add configuration field to PipelineConfig
struct PipelineConfig {
    // existing fields...
    my_threshold: f64,
}

// Add pipeline stage after correlation analysis
fn run_my_analysis(
    df: &DataFrame,
    config: &PipelineConfig,
) -> Result<Vec<String>> {
    let features_to_drop = analyze_my_feature(df, config.my_threshold)?;
    Ok(features_to_drop)
}

// Call in main() pipeline
let features_to_drop_my = run_my_analysis(&df, &config)?;
df = df.drop_many(&features_to_drop_my);
```

4. **Add CLI argument** in `src/cli/args.rs`:

```rust
#[derive(Parser, Debug)]
pub struct Cli {
    // existing fields...

    /// My analysis threshold
    #[arg(long, default_value = "0.5")]
    pub my_threshold: f64,
}
```

5. **Add tests** in `tests/test_my_analysis.rs`:

```rust
use lophi::pipeline::analyze_my_feature;
mod common;
use common::*;

#[test]
fn test_my_analysis() {
    let df = create_test_dataframe();
    let result = analyze_my_feature(&df, 0.5).unwrap();
    assert!(!result.is_empty());
}
```

### Adding a New CLI Option

To add a new command-line option:

1. **Add field to `Cli` struct** in `src/cli/args.rs`:

```rust
#[derive(Parser, Debug)]
pub struct Cli {
    // existing fields...

    /// Description of new option
    #[arg(long, default_value = "default_value")]
    pub my_option: String,
}
```

2. **Add field to `Config` struct** in `src/cli/config_menu.rs`:

```rust
pub struct Config {
    // existing fields...
    pub my_option: String,
}
```

3. **Initialize in `setup_configuration`** in `src/main.rs`:

```rust
let mut config = PipelineConfig {
    // existing fields...
    my_option: cli.my_option.clone(),
};
```

4. **Use in pipeline** wherever needed:

```rust
if config.my_option == "special_mode" {
    // Special handling
}
```

### Adding a New TUI Shortcut

To add a new keyboard shortcut to the interactive configuration menu:

1. **Add menu state** in `src/cli/config_menu.rs`:

```rust
enum MenuState {
    // existing states...
    EditMyOption {
        input: String,
    },
}
```

2. **Add keyboard handler** in the main event loop (around line 540):

```rust
MenuState::Main => match key.code {
    // existing handlers...

    KeyCode::Char('m') | KeyCode::Char('M') => {
        state = MenuState::EditMyOption {
            input: config.my_option.clone(),
        };
    }
}
```

3. **Add state handler** for the new menu state:

```rust
MenuState::EditMyOption { input } => match key.code {
    KeyCode::Enter => {
        // Validate and save input
        config.my_option = input.clone();
        state = MenuState::Main;
    }
    KeyCode::Esc => {
        state = MenuState::Main;
    }
    KeyCode::Backspace => {
        input.pop();
    }
    KeyCode::Char(c) => {
        input.push(c);
    }
    _ => {}
}
```

4. **Update the UI rendering** to display the option:

```rust
fn render_config_panel(/* params */) {
    // Add line to display new option
    format!("My Option:   {}", config.my_option)
}
```

5. **Update help text** in the main menu to document the shortcut:

```rust
"[M] Edit my option"
```

## Additional Resources

- [Architecture](architecture.md) - Detailed module structure and data flow
- [User Guide](user-guide.md) - CLI usage and TUI reference
- [Algorithms](algorithms.md) - Statistical methods and formulas
- [Glossary](glossary.md) - Domain terminology (WoE, IV, Gini, etc.)

## Getting Help

For questions or issues:

1. Check existing documentation in `docs/`
2. Search closed issues on GitHub
3. Open a new issue with:
   - Lo-phi version (`lophi --version`)
   - Rust version (`rustc --version`)
   - Platform (OS, architecture)
   - Minimal reproducible example
