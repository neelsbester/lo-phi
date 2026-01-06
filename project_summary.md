# Lo-phi (φ) - Project Summary

> **A Rust CLI tool for automated feature reduction in machine learning datasets**

## Overview

Lo-phi is a command-line application designed to streamline feature engineering workflows by automatically identifying and removing low-value features from datasets. It targets data scientists and ML engineers who need to reduce dimensionality before model training.

The tool applies three reduction strategies in sequence:
1. **Missing Value Analysis** - Removes features with excessive missing data
2. **Univariate Gini Analysis** - Removes features with low predictive power (via WoE binning)
3. **Correlation Analysis** - Removes redundant features from highly correlated pairs

## Technology Stack

| Component | Library | Purpose |
|-----------|---------|---------|
| Data Processing | `polars` (lazy + streaming) | Memory-efficient large dataset handling with streaming conversion |
| CLI Arguments | `clap` (derive) | Type-safe argument parsing with defaults |
| Progress Bars | `indicatif` | Visual progress indicators during analysis |
| Interactive Prompts | `dialoguer` | Step confirmations and threshold adjustments |
| TUI Framework | `ratatui` + `crossterm` | Interactive configuration menu with ASCII logo |
| Table Output | `comfy-table` | Display reduction summaries |
| Error Handling | `anyhow` + `thiserror` | Clean error messages |
| Parallel Processing | `rayon` | Multi-threaded correlation and IV calculations |
| Terminal Styling | `console` | Colors, emojis, and formatting |
| Serialization | `serde` + `serde_json` | Export Gini/IV analysis to JSON |

## Project Structure

```
lophi/
├── Cargo.toml              # Rust dependencies and project metadata
├── Makefile                # Development commands (test, lint, fmt, etc.)
├── plan.md                 # Architecture diagram and development plan
├── project_summary.md      # This file
├── src/
│   ├── main.rs             # Entry point, CLI setup, main pipeline orchestration
│   ├── lib.rs              # Library exports for testing
│   ├── cli/
│   │   ├── mod.rs          # CLI module exports
│   │   ├── args.rs         # Clap argument definitions
│   │   ├── config_menu.rs  # Interactive TUI configuration menu
│   │   └── convert.rs      # CSV to Parquet conversion subcommand
│   ├── pipeline/
│   │   ├── mod.rs          # Pipeline module exports
│   │   ├── loader.rs       # CSV/Parquet loading with progress
│   │   ├── missing.rs      # Missing value analysis and reduction
│   │   ├── correlation.rs  # Correlation-based reduction
│   │   ├── iv.rs           # Information Value / Gini analysis (WoE binning)
│   │   ├── target.rs       # Target column analysis and mapping
│   │   └── weights.rs      # Sample weight extraction and validation
│   ├── report/
│   │   ├── mod.rs          # Report module exports
│   │   ├── summary.rs      # Reduction summary with timing info
│   │   └── gini_export.rs  # Export Gini analysis to JSON
│   └── utils/
│       └── progress.rs     # Progress bar and spinner helpers
├── tests/
│   ├── common/             # Shared test utilities and fixtures
│   ├── test_cli.rs         # CLI integration tests
│   ├── test_convert.rs     # CSV to Parquet conversion tests
│   ├── test_correlation.rs # Correlation module tests
│   ├── test_loader.rs      # Data loading tests
│   ├── test_missing.rs     # Missing value analysis tests
│   ├── test_pipeline.rs    # End-to-end pipeline tests
│   └── test_target_mapping.rs  # Target column mapping tests
├── scripts/
│   └── generate_test_data.py  # Python script for synthetic test data
└── test_data/              # Generated test datasets (CSV + Parquet)
```

## CLI Interface

### Main Reduce Command

```bash
# Basic usage (interactive mode)
lophi --input data.csv

# With target and output specified
lophi --input data.csv --target target_column --output reduced.parquet

# Non-interactive mode with all options
lophi --input data.parquet \
  --target target_column \
  --output reduced.parquet \
  --missing-threshold 0.3 \
  --gini-threshold 0.05 \
  --gini-bins 10 \
  --correlation-threshold 0.95 \
  --drop-columns "col1,col2,col3" \
  --no-confirm

# Non-binary target (e.g., "G"/"B" or "good"/"bad")
lophi --input data.csv \
  --target status \
  --event-value "B" \
  --non-event-value "G" \
  --no-confirm
```

### CLI Arguments

| Argument | Short | Default | Description |
|----------|-------|---------|-------------|
| `--input` | `-i` | *required* | Input file path (CSV or Parquet) |
| `--target` | `-t` | *interactive* | Target column name (preserved during reduction) |
| `--event-value` | | *interactive* | Value representing EVENT (1) in target column |
| `--non-event-value` | | *interactive* | Value representing NON-EVENT (0) in target column |
| `--output` | `-o` | `{input}_reduced.{ext}` | Output file path |
| `--missing-threshold` | | `0.3` | Drop features with missing values above this ratio |
| `--gini-threshold` | | `0.05` | Drop features with Gini coefficient below this value |
| `--gini-bins` | | `10` | Number of bins for Gini/IV calculation |
| `--binning-strategy` | | `quantile` | Binning strategy: "quantile" (equal-frequency) or "cart" (decision tree) |
| `--min-category-samples` | | `5` | Min samples per category before merging to "OTHER" |
| `--correlation-threshold` | | `0.95` | Drop one feature from pairs above this correlation |
| `--drop-columns` | | *none* | Comma-separated list of columns to drop before analysis |
| `--weight-column` | `-w` | *none* | Column containing sample weights for weighted analysis |
| `--no-confirm` | | `false` | Skip interactive configuration menu |
| `--infer-schema-length` | | `10000` | Rows to use for CSV schema inference |

### Convert Subcommand

```bash
# Convert CSV to Parquet (faster loading, smaller files)
lophi convert input.csv
lophi convert input.csv --output custom_name.parquet
```

**Performance Features:**
- Uses streaming `sink_parquet()` for memory-efficient conversion
- No full dataset materialization - streams directly from CSV to Parquet
- Snappy compression for optimal file sizes
- Optimal row group size (100,000) for query performance

## Core Pipeline

### Step 1: Load Dataset
- Supports CSV and Parquet formats
- Progress bar for large CSV files
- Displays row/column counts and estimated memory usage
- Applies user-specified column drops before analysis

### Step 2: Missing Value Analysis
- Calculates missing value ratio for each feature
- Drops features exceeding the threshold (default: 30%)
- Always preserves the target column

### Step 3: Univariate Gini Analysis
- Calculates Information Value (IV) and Gini coefficient for each feature
- Supports both **numeric** and **categorical** features
- **Binning strategies**:
  - **Quantile** (default): Equal-frequency bins with greedy IV-optimal merging
  - **CART**: Decision tree-based splits that maximize information gain
- **Categorical handling**: Rare categories (< min_category_samples) merged to "OTHER"
- Drops features below the Gini threshold (default: 0.05)
- Exports full analysis with metadata to `{input}_gini_analysis.json`

### Step 4: Correlation Analysis
- Calculates Pearson correlation for all numeric column pairs
- Uses parallel processing via Rayon for large datasets
- For correlated pairs above threshold:
  - Drops the feature appearing in more correlations
  - Never drops the target column

### Step 5: Save Results
- Writes reduced dataset to CSV or Parquet (based on output extension)
- Displays comprehensive reduction summary with timing

## Key Implementation Details

### Memory Efficiency
- Uses Polars LazyFrame for query optimization
- Processes correlation matrix in parallel chunks
- Only materializes data when necessary

### Parallel Processing
- Correlation calculations use Rayon for multi-threaded execution
- IV/Gini analysis runs features in parallel with progress updates
- Welford's algorithm for numerically stable correlation computation

### Gini/IV Calculation

**Binning Strategies:**
- **Quantile binning** (default):
  - Creates 50 initial quantile pre-bins (equal-frequency)
  - Greedy merging to target bin count (minimizes IV loss)
- **CART binning** (decision tree):
  - Recursively finds optimal splits that maximize Gini impurity reduction
  - Produces bins aligned with natural data boundaries
  - More effective for features with non-linear relationships

**Missing Value Handling:**
- Missing/null feature values are placed in a dedicated **MISSING bin** (not dropped)
- The MISSING bin has its own WoE and IV contribution calculated
- Only records with invalid/unmapped target values are excluded from analysis
- This allows understanding the predictive power of "missingness" itself
- Gini calculation includes samples from the MISSING bin
- Exported JSON includes missing bin statistics when present

**Categorical Feature Support:**
- Groups categories and calculates WoE/IV per category
- Null/missing category values go to a separate MISSING bin
- Rare categories (< `--min-category-samples`) merged into "OTHER"
- Gini calculated using category WoE values (including MISSING bin)

**Common:**
- **WoE Convention**: Uses `ln(%bad/%good)` formula where:
  - WoE > 0 indicates higher risk (more events/defaults)
  - WoE < 0 indicates lower risk (fewer events/defaults)
  - This is intuitive for credit scoring: higher WoE = higher risk
- Laplace smoothing to avoid log(0) in WoE calculation
- AUC-based Gini using Mann-Whitney U statistic (positive Gini = good discrimination)
- Enhanced bin statistics: count, population_pct, event_rate

### Target Column Handling
- **Binary targets (0/1)**: Automatically detected and used directly
- **Non-binary targets**: Interactive selection or CLI arguments for event/non-event mapping
- Supports string values (e.g., "G"/"B", "good"/"bad") with user-defined mapping
- Supports numeric non-binary values (e.g., 1/2/3) with selective mapping
- Multi-value targets: Only selected event and non-event values are used in analysis; other records are preserved in output but ignored during Gini/IV calculation
- Floating-point tolerance (1e-9) for schema conversion edge cases
- Clear error messages for empty, all-null, or missing target columns

### Target Mapping Flow
1. After loading data, target column is analyzed for unique values
2. If values are binary 0/1 → proceed directly
3. If values are non-binary:
   - **Interactive mode**: TUI selector prompts for event and non-event values
   - **CLI mode**: Requires `--event-value` and `--non-event-value` arguments
4. During IV/Gini analysis:
   - Rows with event value are treated as 1
   - Rows with non-event value are treated as 0
   - Rows with other values are ignored in analysis but preserved in output

### Correlation Strategy
- Pearson correlation on numeric columns only
- Processes upper triangle of correlation matrix
- Drop selection: features appearing in more pairs are dropped first

### Sample Weights
Lo-phi supports weighted analysis through the `--weight-column` option:

**Usage:**
```bash
lophi --input data.csv --target target --weight-column sample_weight --no-confirm
```

**How Weights Affect Analysis:**
- **Missing Value Analysis**: Uses weighted missing ratio (`weighted_null_count / total_weight`) instead of unweighted ratio
- **Gini/IV Analysis**: WoE bins, IV contributions, and Gini coefficients calculated using weighted event/non-event counts
- **Correlation Analysis**: Weighted Pearson correlation using weighted Welford's algorithm

**Weight Column Behavior:**
- The weight column is automatically **excluded from feature analysis** (not treated as a feature)
- Zero weights effectively exclude samples from calculations
- Negative weights cause an error
- NaN or infinite weights cause an error
- Null weights default to 1.0 with a warning

**Validation:**
- Column must exist in the dataset
- Column must be numeric (castable to Float64)
- All values must be non-negative, finite, and not NaN

## Development Workflow

### Build Commands

```bash
make build        # Debug build
make release      # Release build
make clean        # Clean artifacts
```

### Testing

```bash
make test              # Run all tests
make test-unit         # Unit tests only
make test-integration  # Integration tests only
make test-verbose      # Tests with output
make test-one TEST=x   # Run specific test
```

### Code Quality

```bash
make lint       # Run clippy linter
make fmt        # Format code
make check-fmt  # Check formatting
make check      # Full CI check (fmt + lint + test)
```

### Test Data Generation

```bash
make gen-test-data        # Small dataset (1K rows, ~60 cols)
make gen-test-data-large  # Large dataset (100K rows, ~5K cols)
make run-test             # Run tool on test data
```

## Output Files

When running the reduction pipeline, Lo-phi generates:

1. **Reduced Dataset** (`{input}_reduced.{csv|parquet}`)
   - The dataset with dropped features removed
   - Same format as specified in output path

2. **Gini Analysis JSON** (`{input}_gini_analysis.json`)
   - **Metadata**: timestamp, lo-phi version, input file, binning strategy, thresholds
   - **Summary**: total features analyzed, numeric/categorical counts, avg IV/Gini
   - **Per-feature analysis**:
     - Feature name and type (Numeric/Categorical)
     - WoE bins (numeric) or categories (categorical) with statistics
     - IV, Gini, population percentages, event rates
     - Dropped status

## Example Output

```
╭──────────────────────────────────────────────────────────────╮
│   Lo-phi v0.1.0                                              │
│   Feature Reduction Tool                                     │
╰──────────────────────────────────────────────────────────────╯

   ◆ Configuration
     Input:    test_data/small_test.parquet
     Target:   target
     Output:   test_data/small_test_reduced.parquet
     ─────────────────────────────────
     Missing threshold:     30.0%
     Gini threshold:        0.05
     Correlation threshold: 0.95

   ✓ Dataset loaded
    ✧ Dataset Statistics:
      Rows: 1000
      Columns: 69

   ◆ Step 1: Missing Value Analysis
   ✓ Missing value analysis complete
      3 feature(s) with high missing values (>30.0%)
   ✓ Dropped features with high missing values
   
   ◆ Step 2: Univariate Gini Analysis
   ✓ Gini analysis saved to test_data/small_test_gini_analysis.json
      12 feature(s) with low Gini (<0.05)
   ✓ Dropped low Gini features

   ◆ Step 3: Correlation Analysis
   ✓ Correlation analysis complete
      5 correlated pair(s) (>0.95)
      Dropping 5 feature(s)
   ✓ Dropped highly correlated features

   ◆ Step 4: Save Results
   ✓ Saved to test_data/small_test_reduced.parquet

    ✦ REDUCTION SUMMARY
    ──────────────────────────────────────────────────────
    │ Metric                │ Value │
    │ ❮ Initial Features    │ 69    │
    │ ✗ Dropped (Missing)   │ 3     │
    │ ◈ Dropped (Low Gini)  │ 12    │
    │ ⋈ Dropped (Correlation)│ 5     │
    │ ✓ Final Features      │ 49    │
    │ ↓ Reduction           │ 29.0% │

 ✓ Feature reduction complete!
```

## Future Extensibility

The pipeline architecture supports adding new reduction steps:

- **IV-based selection**: Already implemented via `pipeline/iv.rs`
- **Variance threshold**: Could be added as another reduction step
- **Custom filters**: Modular design allows plugging in new strategies

Each step follows the pattern:
1. Analyze features against criteria
2. Identify features to drop
3. Update summary statistics
4. Drop features from DataFrame



