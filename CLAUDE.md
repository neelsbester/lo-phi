# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Lo-phi is a Rust CLI tool for feature reduction in datasets. It analyzes features based on three criteria:
1. **Missing values** - drops columns exceeding a null ratio threshold
2. **Gini/IV (Information Value)** - drops features with low predictive power using WoE binning
3. **Correlation** - drops one feature from highly correlated pairs

## Documentation

Comprehensive project documentation is available in `docs/`:

- `docs/glossary.md` - Domain terminology (WoE, IV, Gini, CART, etc.)
- `docs/architecture.md` - System architecture and module structure
- `docs/algorithms.md` - Statistical methods and formulas with source code verification
- `docs/user-guide.md` - CLI arguments, TUI shortcuts, and common workflows
- `docs/developer-guide.md` - Build, test, benchmark, CI, and contributing guide
- `docs/output-reference.md` - Output file formats and JSON schemas
- `docs/worked-example.md` - End-to-end pipeline walkthrough with synthetic data
- `docs/adr/` - 8 Architectural Decision Records (ADR-001 through ADR-008)

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

- **`src/cli/`** - CLI argument parsing (`args.rs`), interactive TUI menu (`config_menu.rs`), CSV/SAS7BDAT-to-Parquet conversion (`convert.rs`)
- **`src/pipeline/`** - Core analysis algorithms:
  - `loader.rs` - CSV/Parquet/SAS7BDAT loading with progress
  - `missing.rs` - Null ratio calculation per column
  - `iv.rs` - WoE/IV binning analysis (most complex module, ~2600 lines)
  - `correlation.rs` - Pearson correlation with Welford algorithm
  - `target.rs` - Binary/non-binary target column handling
  - `sas7bdat/` - Pure Rust SAS7BDAT binary file parser (see below)
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

- `DEFAULT_PREBINS = 20` - Initial quantile bins before merging
- `MIN_BIN_SAMPLES = 5` - Minimum samples per bin
- `SMOOTHING = 0.5` - Laplace smoothing to prevent log(0)

### SAS7BDAT Parser (`src/pipeline/sas7bdat/`)

Pure Rust parser for SAS7BDAT binary files (read-only). No external C/FFI dependencies.

**Module structure:**
- `mod.rs` - Public API: `load_sas7bdat(path)`, `get_sas7bdat_columns(path)`, core type definitions
- `constants.rs` - Magic numbers, offsets, page types, subheader signatures, encoding map, epoch constants
- `error.rs` - `SasError` enum with 9 variants (InvalidMagic, TruncatedFile, ZeroRows, etc.)
- `header.rs` - File header parsing (alignment, endianness, encoding, page/row dimensions)
- `page.rs` - Page header parsing and type classification (Meta, Data, Mix, AMD, Comp)
- `subheader.rs` - Subheader pointer table and metadata extraction (RowSize, ColumnSize, ColumnText, ColumnName, ColumnAttributes, FormatAndLabel)
- `column.rs` - Column metadata construction, format-to-Polars type inference, encoding-aware text decoding
- `decompress.rs` - RLE (16 control byte commands) and RDC (LZ77 sliding window) decompression
- `data.rs` - Row extraction, truncated numeric reconstruction, missing value detection, date/time epoch conversion, character encoding via `encoding_rs`

**Key types:**
```rust
SasDataType { Numeric, Character }
PolarsOutputType { Float64, Date, Datetime, Time, Utf8 }
Compression { None, Rle, Rdc }
SasEncoding { Utf8, Ascii, Latin1, Windows1252, Other { id, name }, Unspecified }
```

**Epoch conversion constants:**
- SAS date epoch: 1960-01-01 (offset: 3653 days to Unix epoch)
- SAS datetime epoch: 1960-01-01 00:00:00 (offset: 315,619,200 seconds to Unix epoch)
- Missing values: 28 sentinel patterns (standard `.` plus `.A`-`.Z` and `._`)

**Integration points:**
- `loader.rs` - `"sas7bdat"` arms in `get_column_names()` and `load_dataset_with_progress()`
- `main.rs` - SAS7BDAT input defaults output extension to `.parquet`
- `config_menu.rs` - `is_valid_data_file()` accepts `.sas7bdat`
- `convert.rs` - `run_convert()` detects SAS7BDAT and routes to `run_convert_sas7bdat()`
- `args.rs` - CLI help text updated for SAS7BDAT support

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
- **good_lp (HiGHS)** - MIP solver for optimal binning with monotonicity constraints
- **faer** - Pure-Rust linear algebra for matrix-based correlation computation
- **encoding_rs** - Character encoding conversion for SAS7BDAT file support

### Architectural Decision Records (ADRs)

Lo-phi's design decisions are documented in `/docs/adr/`:

- **ADR-001**: Polars DataFrame Framework - Why Polars over pandas/DataFusion/ndarray
- **ADR-002**: HiGHS MIP Solver - Why HiGHS via good_lp over CBC/GLPK/custom solver
- **ADR-003**: CART Default Binning - Why CART over quantile/fixed-width binning
- **ADR-004**: WoE Convention ln(Bad/Good) - Why this sign convention over alternatives
- **ADR-005**: Welford Correlation Algorithm - Why Welford over two-pass/naive formulas
- **ADR-006**: Sequential Pipeline Stages - Why Missing→Gini→Correlation ordering
- **ADR-007**: Dual CSV/Parquet Support - Why both formats over CSV-only/Parquet-only
- **ADR-008**: Ratatui Terminal UI - Why Ratatui over CLI-only/web UI/GUI

Each ADR documents context, decision rationale, alternatives considered, and consequences.

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
- `[F]` - Convert CSV/SAS7BDAT to Parquet (fast in-memory mode)
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
