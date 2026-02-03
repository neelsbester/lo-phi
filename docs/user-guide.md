# User Guide

This guide covers everything you need to know about using Lo-phi, from installation to advanced configuration options.

## Installation and Quick Start

### Prerequisites

Lo-phi requires the Rust toolchain (1.70 or later). Install Rust from [rustup.rs](https://rustup.rs/).

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/lo-phi.git
cd lo-phi

# Build release binary
cargo build --release

# The binary will be at target/release/lophi
```

### Basic Usage

Run Lo-phi in interactive mode by executing the binary without arguments:

```bash
lophi
```

This launches the interactive TUI menu where you can select a file, configure parameters, and run the reduction pipeline.

For non-interactive use, specify the input file and target column:

```bash
lophi --input data.csv --target default_flag
```

The tool analyzes features using three criteria: missing value ratio, [Gini coefficient](glossary.md#gini-coefficient)/[IV](glossary.md#information-value-iv), and correlation. Features failing any threshold are dropped. See [algorithms](algorithms.md) for details on how each analysis works.

## CLI Mode Reference

### Main Command Arguments

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--input`, `-i` | Path | Required* | Input CSV or Parquet file (*or selected via file selector) |
| `--target`, `-t` | String | Interactive | Target column name (binary or mappable to 0/1) |
| `--output`, `-o` | Path | `{input}_reduced.{ext}` | Output file path for reduced dataset |
| `--missing-threshold` | Float | 0.3 | Drop features with missing ratio above this value (0.0-1.0) |
| `--gini-threshold` | Float | 0.05 | Drop features with [Gini](glossary.md#gini-coefficient) below this value (0.0-1.0) |
| `--correlation-threshold` | Float | 0.40 | Drop one feature from pairs with correlation above this value (0.0-1.0) |
| `--gini-bins` | Integer | 10 | Number of bins for Gini/IV calculation |
| `--binning-strategy` | String | "cart" | Binning method: "cart" (decision tree splits) or "quantile" (equal-frequency) |
| `--prebins` | Integer | 20 | Initial bins before optimization/merging. Lower = faster, higher = more precise solver |
| `--use-solver` | Boolean | true | Enable MIP solver for optimal binning (see [algorithms](algorithms.md#solver-based-binning-optimization)) |
| `--monotonicity` | String | "none" | WoE monotonicity constraint: "none", "ascending", "descending", "peak", "valley", "auto" |
| `--solver-timeout` | Integer | 30 | Maximum solver time per feature (seconds) |
| `--solver-gap` | Float | 0.01 | MIP gap tolerance (0.0-1.0). Lower = more precise but slower |
| `--cart-min-bin-pct` | Float | 5.0 | Minimum bin size as percentage of total samples for CART binning (0.0-100.0) |
| `--min-category-samples` | Integer | 5 | Minimum samples per category. Categories below this are merged into "OTHER" |
| `--event-value` | String | None | Value in target representing EVENT (maps to 1). Required with `--non-event-value` for non-binary targets |
| `--non-event-value` | String | None | Value in target representing NON-EVENT (maps to 0). Required with `--event-value` for non-binary targets |
| `--weight-column`, `-w` | String | None | Column containing sample weights. Enables [weighted analysis](glossary.md#weighted-analysis) |
| `--drop-columns` | String | None | Comma-separated columns to drop before analysis (e.g., "id,timestamp") |
| `--infer-schema-length` | Integer | 10000 | Rows to scan for CSV schema inference. Use 0 for full scan (slow) |
| `--no-confirm` | Boolean | false | Skip interactive confirmation prompts |

### Example Commands

**Basic analysis with defaults:**
```bash
lophi --input creditdata.csv --target default_flag
```

**Custom thresholds:**
```bash
lophi --input data.parquet --target outcome \
  --missing-threshold 0.20 \
  --gini-threshold 0.10 \
  --correlation-threshold 0.50
```

**Weighted analysis with monotonic binning:**
```bash
lophi --input survey.csv --target response \
  --weight-column sample_weight \
  --monotonicity ascending \
  --gini-bins 8
```

**Non-binary target mapping:**
```bash
lophi --input data.csv --target status \
  --event-value "approved" \
  --non-event-value "rejected"
```

**Drop specific columns before processing:**
```bash
lophi --input raw.parquet --target label \
  --drop-columns "id,created_at,user_id"
```

**Disable solver for faster runtime:**
```bash
lophi --input large_dataset.parquet --target flag \
  --use-solver false
```

### Convert Subcommand

Convert CSV files to Parquet format with optimized compression and schema inference.

```bash
lophi convert <INPUT> [OPTIONS]
```

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `input` | Path | Required | Input CSV file |
| `output` | Path | `{input}.parquet` | Output Parquet file path |
| `--infer-schema-length` | Integer | 10000 | Rows to scan for schema inference. Use 0 for full scan |
| `--fast` | Boolean | false | Use in-memory conversion (faster, uses more RAM). Default is streaming mode (slower, low memory) |

**Example:**
```bash
# Streaming mode (low memory, single-threaded)
lophi convert data.csv

# Fast mode (high memory, parallel)
lophi convert data.csv --fast
```

Fast mode loads the entire dataset into memory and parallelizes column encoding across all CPU cores. Streaming mode processes data in chunks with minimal RAM usage but runs single-threaded. See [CSV to Parquet Conversion](#csv-to-parquet-conversion) for details.

## Interactive TUI Mode

### Launching Interactive Mode

When you run `lophi` without arguments, it launches the interactive TUI (Text User Interface):

```bash
lophi
```

The interface displays a three-column layout showing all current configuration values:

```
  THRESHOLDS          │  SOLVER            │  DATA
  Missing:     0.30   │  Solver: Yes       │  Drop:    None
  Gini:        0.05   │  Trend:  none      │  Weight:  None
  Correlation: 0.40   │                    │  Schema:  10000
```

### Keyboard Shortcuts

**Main Menu Navigation:**

| Key | Action |
|-----|--------|
| `Enter` | Run pipeline with current settings (requires target selected) |
| `T` | Select target column |
| `F` | Convert CSV to Parquet (launches converter) |
| `D` | Select columns to drop before analysis |
| `C` | Edit thresholds (Missing → Gini → Correlation in sequence) |
| `S` | Edit solver options (toggle solver → select monotonicity) |
| `W` | Select weight column for weighted analysis |
| `A` | Advanced options (schema inference length) |
| `Q` or `Esc` | Quit |
| `↑`/`↓` or `k`/`j` | Scroll menu content |
| `PageUp`/`PageDown` | Scroll by page |
| `Home` | Jump to top |

**Popup Dialog Navigation:**

All popup dialogs share consistent navigation:

| Key | Action |
|-----|--------|
| `Enter` | Confirm selection |
| `Esc` | Cancel and return to main menu |
| `↑`/`↓` | Navigate list items |
| `Backspace` | Delete character in search/input fields |
| `Space` | Toggle checkbox (in multi-select dialogs) |
| `Tab` | Cycle through fields (in threshold editor) |

### TUI Dialogs

The TUI provides seven specialized popup dialogs:

1. **Target Column Selector** (`T` key)
   - Searchable list of all columns
   - Type to filter columns by name
   - Press `Enter` to select

2. **Drop Columns Selector** (`D` key)
   - Multi-select checkbox list
   - Press `Space` to toggle checkboxes
   - Shows selected count in header

3. **Threshold Editor** (`C` key)
   - Chain of three input fields: Missing → Gini → Correlation
   - Press `Enter` or `Tab` to advance to next threshold
   - Press `Esc` to cancel at any step

4. **Solver Toggle** (`S` key, first step)
   - Toggle solver on/off
   - Press `Space` or `Enter` to toggle
   - If enabled, automatically opens monotonicity selector

5. **Monotonicity Selector** (`S` key, second step)
   - Select WoE trend constraint
   - Options: none, ascending, descending, peak, valley, auto
   - Only shown when solver is enabled

6. **Weight Column Selector** (`W` key)
   - Searchable list with "None" option
   - Select "None" to disable weighted analysis

7. **Schema Inference Editor** (`A` key)
   - Input field for number of rows to scan
   - Enter "0" for full table scan (slow for large files)

### Navigation Tips

- The main menu scrolls automatically when content exceeds window height
- A scrollbar appears on the right edge when scrolling is active
- Scroll position is preserved when opening/closing dialogs
- Target selection is required before running the pipeline (Enter key is grayed out until target is selected)

## Configuration Parameters

### TUI-Configurable vs CLI-Only

Parameters fall into two categories:

**Configurable via TUI and CLI:**
- Thresholds: missing, gini, correlation
- Solver: use solver toggle, monotonicity constraint
- Data: columns to drop, weight column, schema inference length

**CLI-Only (use sensible defaults in TUI):**
- Binning details: `--binning-strategy`, `--gini-bins`, `--prebins`
- CART parameters: `--cart-min-bin-pct`
- Categorical handling: `--min-category-samples`
- Solver tuning: `--solver-timeout`, `--solver-gap`

The TUI provides the most commonly adjusted parameters. For fine-grained binning control, use CLI mode.

### Parameter Effects

**Missing Threshold** (default: 0.30)
- Features with missing ratio > threshold are dropped
- Example: 0.30 means drop if >30% of rows are null
- Lower values = stricter (drop more features)
- See [Missing Value Analysis](algorithms.md#missing-value-analysis)

**Gini Threshold** (default: 0.05)
- Features with [Gini coefficient](glossary.md#gini-coefficient) < threshold are dropped
- Gini measures predictive power via [WoE binning](glossary.md#weight-of-evidence-woe)
- Higher values = stricter (drop more features)
- See [Gini/IV Analysis](algorithms.md#information-value-iv-calculation)

**Correlation Threshold** (default: 0.40)
- From each pair with correlation > threshold, drop the feature with lower Gini
- Higher values = less aggressive (keep more features)
- Only applies to numeric features
- See [Correlation Analysis](algorithms.md#pearson-correlation)

**Solver** (default: enabled)
- When enabled, uses MIP optimization for globally optimal bin boundaries
- When disabled, uses greedy merging algorithm (faster but suboptimal)
- Solver can enforce monotonicity constraints
- See [Optimal Binning Solver](algorithms.md#solver-based-binning-optimization)

**Monotonicity Constraint** (default: "none")
- `none`: No constraint on WoE pattern
- `ascending`: WoE increases with bin index
- `descending`: WoE decreases with bin index
- `peak`: WoE increases then decreases
- `valley`: WoE decreases then increases
- `auto`: Automatically selects best pattern based on data
- Only enforced when solver is enabled

**Weight Column** (default: None)
- When specified, all statistics use weighted calculations
- Affects missing ratio, Gini/IV, and correlation
- Weights must be numeric and non-negative
- See [Weighted Analysis](glossary.md#weighted-analysis)

**Schema Inference Length** (default: 10000)
- Number of rows to scan when inferring CSV column types
- Higher values improve type detection for ambiguous columns
- Use 0 for full scan (accurate but very slow for large files)
- Only affects CSV input (Parquet has embedded schema)

## CSV to Parquet Conversion

Lo-phi provides built-in CSV to Parquet conversion with two modes optimized for different use cases.

### Using the TUI

Press `F` in the interactive menu to launch the converter. It uses fast in-memory mode by default.

### Using the CLI

```bash
lophi convert input.csv [--output output.parquet] [--fast] [--infer-schema-length 10000]
```

### Conversion Modes

**Streaming Mode** (default):
- Memory-efficient: processes data in chunks
- Single-threaded
- Best for: Large files (>2GB), machines with limited RAM
- Speed: Moderate

**Fast Mode** (`--fast` flag):
- Loads entire dataset into memory
- Parallelizes column encoding across all CPU cores
- Best for: Smaller files (<2GB), machines with sufficient RAM
- Speed: 2-5x faster than streaming mode
- RAM requirement: Roughly 2-3x the CSV file size

### Conversion Features

- Automatic schema inference with configurable row sampling
- Snappy compression for optimal read performance
- Row groups sized at 100,000 rows for efficient querying
- Full column statistics for query optimization
- Typical file size reduction: 40-70% smaller than CSV

**Example output:**
```
Converting CSV to Parquet [fast (in-memory, multi-core)]
   Input:  data.csv
   Output: data.parquet
   Schema inference: 10000 rows

   1,000,000 rows × 50 columns
   File sizes:
      CSV:     1024.00 MB
      Parquet: 312.45 MB
      ↓ 69.5% smaller

   Timing breakdown:
      Init:    0.12s
      Schema:  0.34s
      Load:    2.87s
      Write:   1.45s
      Total:   4.78s
      Throughput: 214.2 MB/s
```

## Common Workflows

### Quick Analysis with Defaults

For initial exploration, use defaults on a sample:

```bash
lophi --input sample.csv --target outcome
```

This applies 30% missing threshold, 0.05 Gini threshold, and 0.40 correlation threshold with optimal binning.

### Custom Threshold Tuning

After reviewing the reduction report, adjust thresholds to control aggressiveness:

```bash
# More conservative (keep more features)
lophi --input data.csv --target label \
  --missing-threshold 0.50 \
  --gini-threshold 0.02 \
  --correlation-threshold 0.60

# More aggressive (drop more features)
lophi --input data.csv --target label \
  --missing-threshold 0.10 \
  --gini-threshold 0.15 \
  --correlation-threshold 0.30
```

Review the `*_reduction_report.csv` to see which features were dropped and why.

### Solver-Based Monotonic Binning

For credit scoring or risk modeling where monotonicity is expected:

```bash
lophi --input creditdata.csv --target default_flag \
  --monotonicity ascending \
  --gini-bins 8 \
  --prebins 15
```

This enforces ascending WoE (higher bin index = higher default risk). Use `--monotonicity auto` to let the solver choose the best pattern.

### Weighted Analysis

For survey data or imbalanced datasets with importance weights:

```bash
lophi --input survey.csv --target response \
  --weight-column survey_weight
```

All statistics (missing ratio, Gini, correlation) will use weighted calculations. Weights must be numeric and non-negative.

### Full Pipeline Example

Typical workflow combining multiple options:

```bash
# 1. Convert CSV to Parquet for faster processing
lophi convert raw_data.csv --fast

# 2. Run reduction with custom parameters
lophi --input raw_data.parquet --target conversion \
  --drop-columns "id,timestamp" \
  --weight-column sample_weight \
  --missing-threshold 0.25 \
  --gini-threshold 0.08 \
  --correlation-threshold 0.45 \
  --monotonicity auto \
  --output reduced_features.parquet

# 3. Review the reduction report
unzip raw_data_reduction_report.zip
cat raw_data_reduction_report.csv
```

The output includes:
- `reduced_features.parquet` - Dataset with low-value features removed
- `raw_data_reduction_report.zip` - Bundle containing JSON and CSV reports with full analysis details

See the [Output Reference](output-reference.md) for details on report structure and interpretation.

## Next Steps

- **[Algorithms Reference](algorithms.md)** - Deep dive into how each analysis stage works
- **[Output Reference](output-reference.md)** - Understand the generated reports and reduced dataset
- **[Glossary](glossary.md)** - Technical term definitions (WoE, IV, Gini, etc.)
- **[Output Reference](output-reference.md)** - Output file formats and JSON schemas
