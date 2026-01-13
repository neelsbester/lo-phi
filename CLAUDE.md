# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Lo-phi is a Rust CLI tool for feature reduction in datasets. It analyzes features based on three criteria:
1. **Missing values** - drops columns exceeding a null ratio threshold
2. **Gini/IV (Information Value)** - drops features with low predictive power using WoE binning
3. **Correlation** - drops one feature from highly correlated pairs

## Build and Development Commands

```bash
# Build
cargo build              # Debug build
cargo build --release    # Release build

# Testing
cargo test --all-features                    # Run all tests
cargo test --lib --all-features              # Unit tests only
cargo test --test '*' --all-features         # Integration tests only
cargo test --all-features -- --nocapture     # Tests with output visible
cargo test --all-features <TEST_NAME> -- --nocapture  # Single test

# Code quality
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo fmt                # Format code
cargo fmt -- --check     # Check formatting

# Full CI check (format + lint + test)
make check

# Benchmarks
cargo bench
```

## Architecture

### Pipeline Flow

The main reduction pipeline in `src/main.rs` orchestrates these stages sequentially:

```
Configuration → Load Dataset → Missing Analysis → Gini/IV Analysis → Correlation Analysis → Save & Report
```

### Module Structure

- **`src/cli/`** - CLI argument parsing (`args.rs`), interactive TUI menu (`config_menu.rs`), CSV-to-Parquet conversion (`convert.rs`)
- **`src/pipeline/`** - Core analysis algorithms:
  - `loader.rs` - CSV/Parquet loading with progress
  - `missing.rs` - Null ratio calculation per column
  - `iv.rs` - WoE/IV binning analysis (most complex module, ~600 lines)
  - `correlation.rs` - Pearson correlation with Welford algorithm
  - `target.rs` - Binary/non-binary target column handling
- **`src/report/`** - Results summary tables (`summary.rs`), Gini JSON export (`gini_export.rs`), comprehensive reduction report (`reduction_report.rs`)
- **`src/utils/`** - Progress bars and terminal styling

### Key Types in `src/pipeline/iv.rs`

```rust
BinningStrategy::Cart      // Decision-tree splits (default)
BinningStrategy::Quantile  // Equal-frequency binning

IvAnalysis {
    feature_name, feature_type,
    bins: Vec<WoeBin>,           // Numeric features
    categories: Vec<CategoricalWoeBin>,  // Categorical features
    iv: f64, gini: f64,
}
```

### Constants in IV Analysis

- `PRE_BIN_COUNT = 50` - Initial quantile bins before merging
- `MIN_BIN_SAMPLES = 5` - Minimum samples per bin
- `SMOOTHING = 0.5` - Laplace smoothing to prevent log(0)

### Test Structure

- `tests/common/mod.rs` - Shared fixtures (`create_test_dataframe()`, temp file helpers, assertion helpers)
- Integration tests: `test_pipeline.rs`, `test_missing.rs`, `test_correlation.rs`, `test_target_mapping.rs`, etc.
- Benchmarks: `benches/binning_benchmark.rs` - Quantile vs CART performance comparison

### Output Files

When running the pipeline, Lo-phi generates the following output files:

1. **`{input}_reduced.{csv|parquet}`** - The reduced dataset with dropped features removed
2. **`{input}_reduction_report.zip`** - Bundled reports containing:
   - `{input}_gini_analysis.json` - Detailed Gini/IV analysis with WoE bins per feature
   - `{input}_reduction_report.json` - Comprehensive JSON report with full analysis details
   - `{input}_reduction_report.csv` - Human-readable CSV summary with one row per feature, including all correlated features (pipe-separated format: `feature: 0.92 | feature2: 0.88`)

### Key Dependencies

- **Polars** - DataFrame operations (lazy/streaming, CSV, Parquet)
- **Rayon** - Parallel processing for correlation and IV analysis
- **Ratatui/Crossterm** - Interactive TUI configuration menu and file selector
- **Indicatif** - Progress bars
- **zip** - Packaging reduction reports into zip archives

### Interactive TUI Options

The interactive configuration menu (`src/cli/config_menu.rs`) provides keyboard shortcuts to configure pipeline options.

**Three-Column Layout:**
```
  THRESHOLDS          │  SOLVER            │  DATA
  Missing:     0.30   │  Solver: Yes       │  Drop:    None
  Gini:        0.05   │  Trend:  none      │  Weight:  None
  Correlation: 0.40   │                    │  Schema:  10000
```

**Keyboard Shortcuts:**
- `[Enter]` - Run with current settings (requires target selected)
- `[T]` - Select target column
- `[D]` - Select columns to drop (now in DATA column)
- `[C]` - Edit thresholds (Missing → Gini → Correlation, chained flow)
- `[S]` - Edit solver options (Use Solver toggle → Trend/Monotonicity selection)
- `[W]` - Select weight column
- `[A]` - Advanced options (Schema inference length)
- `[Q]` - Quit

**TUI-Configurable Parameters:**
| Category | Parameter | Default | Range |
|----------|-----------|---------|-------|
| Thresholds | Missing | 0.30 | 0.0-1.0 |
| Thresholds | Gini | 0.05 | 0.0-1.0 |
| Thresholds | Correlation | 0.40 | 0.0-1.0 |
| Solver | Use Solver | true | true/false |
| Solver | Trend (monotonicity) | none | none, ascending, descending, peak, valley, auto |
| Data | Drop columns | None | column names |
| Data | Weight Column | None | column name or None |
| Data | Schema Inference | 10000 | 100+ rows (0 = full scan) |

**CLI-Only Parameters (not in TUI):**
Binning parameters use sensible defaults and are only configurable via CLI:
- `--binning-strategy` (default: cart)
- `--gini-bins` (default: 10)
- `--prebins` (default: 20)
- `--cart-min-bin-pct` (default: 5.0)
- `--min-category-samples` (default: 5)
- `--solver-timeout` (default: 30s)
- `--solver-gap` (default: 0.01)

## Future Enhancements (TODO)

### Correlation Analysis Improvements

1. **WoE-Encoded Correlation for Numeric Features**
   - Current: Correlation uses raw feature values
   - Proposed: Option to use WoE-encoded values for correlation
   - Benefit: Measures correlation in "predictive space" rather than raw linear relationships
   - Infrastructure exists: `find_woe_for_value()` in `iv.rs` already maps values to WoE
   - Consideration: Results become dependent on binning parameters

2. **Cramér's V for Categorical Features**
   - Current: Categorical features are excluded from correlation analysis
   - Proposed: Use Cramér's V to detect association between categorical pairs
   - Formula: `V = sqrt(χ² / (N × (k-1)))` where k = min(categories_A, categories_B)
   - Benefit: Identifies redundant categorical features (e.g., `city` and `postal_code`)
   - Consideration: Handle missing values as "MISSING" category (consistent with IV analysis)
