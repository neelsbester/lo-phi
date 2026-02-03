# Lo-phi

A Rust CLI tool for intelligent feature reduction in datasets. Lo-phi analyzes and removes features based on missing values, predictive power (Gini/IV), and correlation, helping you build cleaner datasets for machine learning.

## Quick Start

### Installation

```bash
cargo install --path .
```

### Basic Usage

Lo-phi offers three usage modes to fit your workflow:

#### 1. Wizard Mode (Recommended for New Users)

The easiest way to get started. The wizard guides you step-by-step through configuration:

```bash
# Launch interactive wizard
lo-phi

# Or start with a specific file
lo-phi data.parquet
```

The wizard will walk you through:
- Selecting your input file (CSV or Parquet)
- Choosing the target column for analysis
- Configuring thresholds (missing values, Gini, correlation)
- Setting optional parameters (solver, weights, columns to drop)
- Reviewing and confirming your settings

**Perfect for:** Exploratory analysis, learning the tool, one-off feature reduction tasks

#### 2. Dashboard Mode (Power Users)

View and adjust all options at once with keyboard shortcuts:

```bash
# Open the dashboard
lo-phi --manual

# Or with a pre-selected file
lo-phi --manual data.csv
```

**Keyboard Shortcuts:**
- `[T]` Select target column
- `[C]` Edit thresholds (Missing, Gini, Correlation)
- `[S]` Configure solver options
- `[W]` Select weight column
- `[D]` Choose columns to drop
- `[F]` Convert CSV to Parquet
- `[A]` Advanced options
- `[Enter]` Run analysis
- `[Q]` Quit

**Perfect for:** Experienced users, rapid iteration, comparing different configurations

#### 3. CLI-Only Mode (Automation)

Pure command-line mode for scripting and automation:

```bash
lo-phi --no-confirm data.csv \
  --target outcome \
  --missing-threshold 0.3 \
  --gini-threshold 0.05 \
  --correlation-threshold 0.4
```

**Perfect for:** CI/CD pipelines, batch processing, reproducible workflows

### CSV to Parquet Conversion

Lo-phi includes a fast CSV-to-Parquet converter with compression options:

```bash
# Via wizard (option [F] in dashboard or dedicated conversion workflow)
lo-phi
# Select: "Convert CSV to Parquet"

# Via CLI
lo-phi convert input.csv output.parquet --compression snappy
```

Supported compression codecs: `snappy` (default), `gzip`, `lz4`, `zstd`, `none`

## What Lo-phi Does

Lo-phi performs three types of feature reduction:

1. **Missing Value Analysis** - Removes features exceeding a null ratio threshold (default: 30%)
2. **Gini/IV Analysis** - Drops features with low predictive power using Weight of Evidence (WoE) binning
3. **Correlation Analysis** - Eliminates one feature from highly correlated pairs (default: 0.40)

### Output Files

After running feature reduction, Lo-phi generates:

- **`{input}_reduced.{csv|parquet}`** - Your cleaned dataset with dropped features removed
- **`{input}_reduction_report.zip`** - Comprehensive analysis reports:
  - `gini_analysis.json` - Detailed WoE binning and IV scores per feature
  - `reduction_report.json` - Full analysis metadata in JSON format
  - `reduction_report.csv` - Human-readable summary with correlation details

## Configuration Options

### Thresholds

| Parameter | Default | Description |
|-----------|---------|-------------|
| `--missing-threshold` | 0.30 | Drop features with >30% missing values |
| `--gini-threshold` | 0.05 | Drop features with Gini coefficient <0.05 |
| `--correlation-threshold` | 0.40 | Drop one feature from pairs with correlation >0.40 |

### Solver Options

| Parameter | Default | Description |
|-----------|---------|-------------|
| `--use-solver` | true | Enable MILP optimizer for optimal binning boundaries |
| `--solver-trend` | none | Monotonicity constraint: `none`, `ascending`, `descending`, `peak`, `valley`, `auto` |
| `--solver-timeout` | 30s | Maximum time for solver optimization |
| `--solver-gap` | 0.01 | MIP gap tolerance (1% optimality) |

### Binning Options

| Parameter | Default | Description |
|-----------|---------|-------------|
| `--binning-strategy` | cart | Binning method: `cart` (decision tree) or `quantile` |
| `--gini-bins` | 10 | Target number of bins for Gini analysis |
| `--prebins` | 20 | Initial bins before merging (quantile strategy) |
| `--cart-min-bin-pct` | 5.0 | Minimum percentage of samples per bin (CART) |
| `--min-category-samples` | 5 | Minimum samples for categorical bin |

### Data Options

| Parameter | Default | Description |
|-----------|---------|-------------|
| `--target` | (required) | Binary target column for predictive analysis |
| `--weight` | None | Optional weight column for weighted statistics |
| `--drop-columns` | None | Comma-separated list of columns to exclude |
| `--schema-inference-length` | 10000 | Rows to scan for type inference (0 = full scan) |

## Examples

### Example 1: Basic Feature Reduction

```bash
# Using wizard mode
lo-phi credit_data.csv

# Using CLI mode with custom thresholds
lo-phi --no-confirm credit_data.csv \
  --target default \
  --missing-threshold 0.25 \
  --gini-threshold 0.03 \
  --correlation-threshold 0.5
```

### Example 2: Weighted Analysis with Solver

```bash
lo-phi --no-confirm survey_data.parquet \
  --target churned \
  --weight sample_weight \
  --use-solver \
  --solver-trend auto
```

### Example 3: Drop Specific Columns

```bash
lo-phi --no-confirm transactions.csv \
  --target fraud \
  --drop-columns transaction_id,timestamp,customer_name
```

### Example 4: Custom Binning Strategy

```bash
lo-phi --no-confirm income_data.csv \
  --target high_earner \
  --binning-strategy quantile \
  --prebins 30 \
  --gini-bins 15
```

## Development

### Build and Test

```bash
# Build
cargo build --release

# Run tests
cargo test --all-features

# Run specific test with output
cargo test --all-features test_missing_analysis -- --nocapture

# Lint and format
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt

# Full CI check
make check
```

### Benchmarks

```bash
cargo bench
```

## Technical Details

### Architecture

Lo-phi uses a pipeline architecture with three sequential analysis stages:

```
Load Dataset → Missing Analysis → Gini/IV Analysis → Correlation Analysis → Save & Report
```

**Key Technologies:**
- **Polars** - High-performance DataFrames with lazy/streaming evaluation
- **Rayon** - Parallel processing for IV and correlation calculations
- **Ratatui/Crossterm** - Interactive terminal UI for wizard and dashboard
- **HiGHS Solver** - MILP optimization for monotonic binning constraints

### WoE Binning

Lo-phi uses Weight of Evidence (WoE) binning to measure feature predictive power:

- **CART Strategy (default):** Decision tree-based splits optimizing Gini impurity
- **Quantile Strategy:** Equal-frequency binning with optional monotonicity constraints
- **Solver Integration:** MILP optimizer enforces monotonic WoE trends while maximizing IV

**Key Constants:**
- Pre-bin count: 50 initial quantiles before merging
- Min bin samples: 5 observations minimum per bin
- Laplace smoothing: 0.5 to prevent log(0) errors

### Correlation Analysis

Pearson correlation computed using Welford's online algorithm for numerical stability. Categorical features are currently excluded (see Future Enhancements).

## Future Enhancements

### Planned Features

1. **WoE-Encoded Correlation** - Measure correlation in "predictive space" using WoE-transformed values instead of raw features
2. **Cramér's V for Categoricals** - Detect association between categorical feature pairs (e.g., city vs postal_code)
3. **Feature Importance Export** - Generate ranked feature importance lists for external modeling tools

## License

[Add your license here]

## Contributing

[Add contribution guidelines here]
