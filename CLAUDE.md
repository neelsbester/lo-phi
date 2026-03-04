# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Lo-phi is a Rust CLI tool for feature reduction and dataset sampling. It supports three workflows:

**Feature Reduction** - analyzes features based on three criteria:
1. **Missing values** - drops columns exceeding a null ratio threshold
2. **Gini/IV (Information Value)** - drops features with low predictive power using WoE binning
3. **Correlation** - drops one feature from highly correlated pairs

**Format Conversion** - bidirectional conversion between CSV, Parquet, and SAS7BDAT formats.

**Dataset Sampling** - produces sampled subsets with inverse probability weights (`sampling_weight` column) for survey-style downstream analysis. Three methods: Random (SRS), Stratified (per-stratum sizes), Equal Allocation (uniform n per stratum).

## Documentation

Comprehensive project documentation is available in `docs/`:

- `docs/glossary.md` - Domain terminology (WoE, IV, Gini, CART, etc.)
- `docs/architecture.md` - System architecture and module structure
- `docs/algorithms.md` - Statistical methods and formulas with source code verification
- `docs/user-guide.md` - CLI arguments, TUI shortcuts, and common workflows
- `docs/developer-guide.md` - Build, test, benchmark, CI, and contributing guide
- `docs/output-reference.md` - Output file formats and JSON schemas
- `docs/worked-example.md` - End-to-end pipeline walkthrough with synthetic data
- `docs/adr/` - 9 Architectural Decision Records (ADR-001 through ADR-009)

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

- **`src/cli/`** - CLI argument parsing (`args.rs`), interactive TUI wizard (`wizard.rs`), dashboard menu (`config_menu.rs`), bidirectional format conversion (`convert.rs`: CSV/SAS7BDAT to Parquet, Parquet to CSV), shared TUI rendering (`shared.rs`: logo, `no_color_mode()`, `themed()`), Catppuccin Mocha theme constants (`theme.rs`: 15 semantic color roles), in-TUI progress overlay (`progress_overlay.rs`: animated pipeline stage display with reduction/sampling/conversion summary on completion; `ProgressOverlay::new()` for reduction, `ProgressOverlay::new_sampling()` for sampling, `ProgressOverlay::new_conversion()` for format conversion; `run_progress_overlay()` accepts an overlay instance)
- **`src/pipeline/`** - Core analysis algorithms:
  - `loader.rs` - CSV/Parquet/SAS7BDAT loading with progress
  - `missing.rs` - Null ratio calculation per column
  - `iv.rs` - WoE/IV binning analysis (most complex module, ~2600 lines)
  - `correlation.rs` - Pearson correlation (num-num, Welford algorithm), bias-corrected Cramér's V (cat-cat), and correlation ratio η/Eta (cat-num); all three measures produce values in [0,1] compared against a single threshold; IV-first drop logic (IV → frequency → missing ratio → alphabetical); high-cardinality guard skips pairs where either categorical has >100 unique values; `_impl` variants accept `silent: bool` to use `ProgressBar::hidden()` in TUI mode
  - `sampling.rs` - Dataset sampling (Random/Stratified/EqualAllocation) with inverse probability weights; types: `SamplingConfig`, `SamplingMethod`, `SampleSize`, `StratumSpec`; public: `analyze_strata()`, `execute_sampling()`
  - `target.rs` - Binary/non-binary target column handling
  - `sas7bdat/` - Pure Rust SAS7BDAT binary file parser (see below)
  - `progress.rs` - Pipeline progress events (`PipelineStage`, `ProgressEvent`, `SummaryData`, `SamplingSummaryData`, `ConversionSummaryData`, `ProgressSender/Receiver` via `mpsc::channel`) for in-TUI progress overlay; `PipelineStage` includes `Sampling` and `Converting` variants for sampling/conversion overlays; pipeline functions have `_with_progress()` variants that send events instead of using indicatif; `SummaryData` carries reduction counts on the `Complete` event; `SamplingSummaryData` carries sampling stats (input/sampled rows, method, output path); `ConversionSummaryData` carries conversion stats (formats, dimensions, file sizes, output path)
- **`src/report/`** - Results summary tables (`summary.rs`), Gini JSON export (`gini_export.rs`), comprehensive reduction report (`reduction_report.rs`)
- **`src/utils/`** - Progress bars and terminal styling (indicatif-based, used in `--no-confirm` CLI mode only)

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

### Key Types in `src/pipeline/correlation.rs`

```rust
AssociationMeasure { Pearson, CramersV, Eta }

FeatureMetadata {
    iv: Option<f64>,
    missing_ratio: Option<f64>,
}

FeatureToDrop {
    feature: String,
    reason: String,
}
```

### Key Types in `src/pipeline/sampling.rs`

```rust
SamplingMethod { Random, Stratified, EqualAllocation }
SampleSize { Count(usize), Fraction(f64) }

StratumSpec {
    value: String,           // stratum identifier
    population_count: usize, // N_h
    sample_size: usize,      // n_h
}

SamplingConfig {
    input, output, method, strata_column,
    sample_size, strata_specs, seed, infer_schema_length,
}
```

### Constants in IV Analysis

- `DEFAULT_PREBINS = 20` - Initial quantile bins before merging
- `MIN_BIN_SAMPLES = 5` - Minimum samples per bin
- `SMOOTHING = 0.5` - Laplace smoothing to prevent log(0)

### SAS7BDAT Parser (`src/pipeline/sas7bdat/`)

Pure Rust parser for SAS7BDAT binary files (read-only). No external C/FFI dependencies.

**Module structure:**
- `mod.rs` - Public API: `load_sas7bdat(path)`, `load_sas7bdat_silent(path)` (TUI-safe, hidden indicatif), `get_sas7bdat_columns(path)`, core type definitions; orchestrates two-pass page iteration (metadata pass + data extraction pass with per-row decompression)
- `constants.rs` - Magic numbers, offsets, page types, subheader signatures, encoding map, epoch constants
- `error.rs` - `SasError` enum with 9 variants (InvalidMagic, TruncatedFile, ZeroRows, etc.)
- `header.rs` - File header parsing (alignment, endianness, encoding, page/row dimensions); magic number validates bytes 12-31 only (bytes 0-11 may vary)
- `page.rs` - Page header parsing and type classification (Meta, Data, Mix, AMD, Comp)
- `subheader.rs` - Subheader pointer table and metadata extraction (RowSize, ColumnSize, ColumnText, ColumnName, ColumnAttributes, FormatAndLabel); FormatAndLabel reads fixed offsets per readstat spec (32-bit: 34/36/38/40/42/44, 64-bit: 46/48/50/52/54/56); entry count uses pandas formula with defensive cap against column_count; compression signature detection at fixed text_block offset 12
- `column.rs` - Column metadata construction, format-to-Polars type inference (30+ SAS date/datetime/time formats including MONYY, E8601DA, DTDATE, TOD, etc.), encoding-aware text decoding via `encoding_rs` (unified with data.rs)
- `decompress.rs` - RLE (16 control byte commands) and RDC (Ross Data Compression / LZ77) decompression; operates per-row (not per-page); accepts `page_index` parameter for accurate error context
- `data.rs` - Row extraction via `extract_rows_from_page` (uncompressed DATA/MIX pages) and `extract_row_values` (public, for individual decompressed row buffers); truncated numeric reconstruction, missing value detection, date/time epoch conversion, character encoding via `encoding_rs`

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
- Missing values: 28 sentinel patterns (standard `.` plus `.A`-`.Z` and `._`); detection validates full 8-byte pattern (sentinel byte + 7 zero bytes) to prevent false positives on legitimate floats; IEEE 754 NaN-encoded missing values caught by secondary `isnan()` check

**Compression architecture (per-row, not per-page):**
- Compressed SAS files store rows as individually compressed subheader entries on META and MIX pages (subheader pointer: `compression == 4`, `subheader_type == 1`)
- Each compressed entry is decompressed to `row_length` bytes using RLE or RDC
- COMP pages (type 0x9000) are padding/marker pages with no useful data -- skipped entirely
- Compression signature detection: always at offset 12 within the ColumnText text_block (offset 16 from subheader start for 32-bit, offset 20 for 64-bit)
- Subheader pointer compression flags: 0=normal metadata, 1=truncated/padding (skip), 4=compressed row data (decompress)
- Uncompressed files: rows live as sequential byte runs in the trailing area of MIX pages or on DATA pages
- Compressed MIX pages: rows extracted exclusively from compressed subheader entries; trailing data area is NOT read (prevents garbage/duplicate row extraction)

**Integration points:**
- `loader.rs` - `"sas7bdat"` arms in `get_column_names()` and `load_dataset_with_progress()`
- `main.rs` - SAS7BDAT input defaults output extension to `.parquet`
- `config_menu.rs` - `is_valid_data_file()` accepts `.sas7bdat`
- `convert.rs` - `run_convert()` routes by input extension: CSV->Parquet, Parquet->CSV (`run_convert_parquet()`), SAS7BDAT->Parquet/CSV (`run_convert_sas7bdat()`)
- `args.rs` - CLI help text updated for SAS7BDAT support

### Test Structure

- `tests/common/mod.rs` - Shared fixtures (`create_test_dataframe()`, temp file helpers, assertion helpers)
- Integration tests: `test_pipeline.rs`, `test_missing.rs`, `test_correlation.rs`, `test_target_mapping.rs`, etc.
- **`tests/test_sas7bdat.rs`** - SAS7BDAT parser integration tests (26 tests): cross-validation against pandas expected outputs, error handling (corrupt/zero_rows/zero_variables/invalid_magic/missing_file), compression equivalence (test1-16), missing value false-positive prevention, Parquet round-trip conversion
- **`tests/fixtures/sas7bdat/expected/`** - JSON metadata + CSV head files generated by pandas for cross-validation
- **`tests/generate_sas_expected.py`** - Python script to regenerate expected outputs from pandas
- **SAS7BDAT test fixtures** (34 files in `tests/fixtures/sas7bdat/`): test1-16 (format variants: 32/64-bit, LE/BE, uncompressed/RLE/RDC), cars, productsales, datetime, many_columns, test_12659, test_meta2_page, zero_rows, zero_variables, airline, 0x40controlbyte, 0x00controlbyte, corrupt, max_sas_date, dates_null, load_log, tagged-na
- **`tests/test_sampling.rs`** - Sampling integration tests (19 tests): random/stratified/equal-allocation sampling, weight verification, edge cases, CSV/Parquet round-trip
- Benchmarks: `benches/binning_benchmark.rs` - Quantile vs CART performance comparison

### Output Files

When running the pipeline, Lo-phi generates the following output files:

1. **`{input}_reduced.{csv|parquet}`** - The reduced dataset with dropped features removed
2. **`{input}_reduction_report.zip`** - Bundled reports containing:
   - `{input}_gini_analysis.json` - Detailed Gini/IV analysis with WoE bins per feature
   - `{input}_reduction_report.json` - Comprehensive JSON report with full analysis details
   - `{input}_reduction_report.csv` - Human-readable CSV summary with one row per feature, including all correlated features (pipe-separated format: `feature: 0.92 | feature2: 0.88`); includes `measure` column (`pearson`, `cramers_v`, `eta`) and `drop_reason` column recording the IV-first drop logic outcome for each correlated pair

When running sampling, Lo-phi generates:

3. **`{input}_sampled.{csv|parquet}`** - The sampled dataset with all original columns plus `sampling_weight` (inverse probability: N/n for random, N_h/n_h per stratum)

### Key Dependencies

- **Polars** - DataFrame operations (lazy/streaming, CSV, Parquet)
- **Rayon** - Parallel processing for correlation and IV analysis
- **Ratatui/Crossterm** - Interactive TUI wizard and dashboard menu with file selector
- **Indicatif** - Progress bars
- **zip** - Packaging reduction reports into zip archives
- **good_lp (HiGHS)** - MIP solver for optimal binning with monotonicity constraints
- **faer** - Pure-Rust linear algebra for matrix-based correlation computation
- **encoding_rs** - Character encoding conversion for SAS7BDAT file support (used in both column metadata and data value decoding)
- **catppuccin** - Catppuccin Mocha color palette with ratatui integration
- **serde_json** (dev) - JSON parsing for SAS7BDAT cross-validation tests

### TUI Theme System (`src/cli/theme.rs`)

The TUI uses a Catppuccin Mocha color palette with 15 semantic color constants:
- **Accents**: `PRIMARY` (Sapphire), `ACCENT` (Mauve), `SUCCESS` (Green), `WARNING` (Yellow), `ERROR` (Red), `DANGER` (Maroon), `KEYS` (Blue)
- **Logo**: `LOGO_LO` (Sky), `LOGO_PHI` (Mauve)
- **Text**: `TEXT`, `SUBTEXT`, `MUTED`
- **Surfaces**: `SURFACE`, `DIVIDER`, `BASE`

All color references in `config_menu.rs` and `wizard.rs` use `theme::CONSTANT` — never hardcoded `Color::` values.

### Accessibility (`src/cli/shared.rs`)

- **`no_color_mode()`** — returns `true` if `NO_COLOR` env var is set (any value) or `TERM=dumb`
- **`themed(style)`** — returns `Style::default()` in no-color mode, otherwise passes through
- **Minimum terminal size** — both wizard and dashboard require 80x24; a centered warning overlay appears if the terminal is resized below this during operation

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
- **ADR-009**: Categorical Association Measures - Cramér's V (cat-cat) and Eta (cat-num) with IV-first drop logic

Each ADR documents context, decision rationale, alternatives considered, and consequences.

### Wizard Mode

Lo-phi provides three distinct usage modes to accommodate different workflows:

**1. Wizard Mode (Default)**
```bash
lo-phi                    # Interactive wizard guides you through configuration
lo-phi data.parquet       # Wizard starts with pre-selected input file
```

The wizard is a step-by-step guided interface that walks users through all configuration options. This is the recommended mode for new users and exploratory analysis.

**2. Dashboard Mode**
```bash
lo-phi --manual           # Opens dashboard with all options visible at once
lo-phi --manual data.csv  # Dashboard with pre-selected input
```

The dashboard presents all configuration options simultaneously with keyboard shortcuts for quick navigation. Ideal for experienced users who want to adjust multiple parameters efficiently.

**3. CLI-Only Mode**
```bash
lo-phi --no-confirm data.csv --target y --missing-threshold 0.3 --gini-threshold 0.05
```

Pure command-line mode bypasses all interactive prompts. Required for scripting, CI/CD pipelines, and batch processing.

**4. Sampling CLI Subcommand**
```bash
lophi sample data.csv -n 100 --seed 42                                      # Random sample
lophi sample data.csv --method stratified --strata-column region \
    --strata-sizes "North:50,South:30,East:20"                               # Stratified
lophi sample data.csv --method equal --strata-column region -n 25            # Equal allocation
```

#### Wizard Flow

**Feature Reduction Workflow (up to 9 steps):**
1. **Select Input File** - File browser with CSV/Parquet/SAS7BDAT filtering
2. **Select Target Column** - Choose target for analysis
3. **Target Mapping** *(conditional)* - For non-binary targets: two-phase UI to assign event (1) and non-event (0) values. Skipped for binary targets.
4. **Configure Thresholds** - Missing (default: 0.30), Gini (0.05), Correlation (0.40)
5. **Solver Options** - Enable optimizer (default: Yes), set monotonicity trend
6. **Weight Column** - Optional sample weights for weighted analysis
7. **Drop Columns** - Multi-select columns to exclude from analysis (selections persist through search filtering)
8. **Advanced Options** - Schema inference row limit (default: 10000)
9. **Confirmation** - Review all settings before execution (scrollable with Up/Down/PageUp/PageDown)

**File Format Conversion Workflow (dynamic steps by input type):**

| Input | Steps Shown | Auto-set |
|-------|-------------|----------|
| **SAS7BDAT** | Select Input File -> **Output Format** (Parquet/CSV) -> Confirmation | `conversion_fast = true` (always in-memory) |
| **CSV** | Select Input File -> **Output Format** (Parquet only) -> Conversion Mode (Fast/Streaming) -> Confirmation | — |
| **Parquet** | Select Input File -> **Output Format** (CSV only) -> Confirmation | `conversion_fast = true` |

1. **Select Input File** - File browser (CSV, Parquet, SAS7BDAT)
2. **Output Format** - Always shown; available options depend on input type (SAS7BDAT: Parquet/CSV, CSV: Parquet only, Parquet: CSV only)
3. **Conversion Mode** - CSV-to-Parquet only: Fast (parallel) vs Memory-efficient (streaming) (auto-skipped otherwise)
4. **Confirmation** - Review conversion settings

**Dataset Sampling Workflow (dynamic steps by method):**

| Method | Steps Shown |
|--------|-------------|
| **Random** | TaskSelection -> SamplingMethod -> SampleSize (count/fraction, Tab to toggle) -> Seed -> Summary |
| **Stratified** | TaskSelection -> SamplingMethod -> StrataColumn -> StratumSizeConfig (per-row n_h) -> Seed -> Summary |
| **Equal** | TaskSelection -> SamplingMethod -> StrataColumn -> SampleSize (n per stratum) -> Seed -> Summary |

- **SamplingMethodSelection**: 3-option list (Random / Stratified / Equal Allocation)
- **SampleSizeInput**: Numeric input with Tab toggle between count and fraction modes
- **StrataColumnSelection**: Search+list UI (reuses TargetSelection pattern)
- **StratumSizeConfig**: Table with editable n_h per row; validates n_h <= N_h inline
- **SeedInput**: Optional numeric seed; empty = random

#### Wizard Architecture

- **Module:** `src/cli/wizard.rs`
- **State Machine:** Each step is an enum variant (`WizardStep`) with associated state
- **Data Accumulation:** Configuration builds incrementally across steps, validated before pipeline execution
- **Integration:** `main.rs` dispatches to wizard by default unless `--manual` or `--no-confirm` flags are present
- **Navigation:** Users can go back to previous steps to revise choices, maintaining consistency across the configuration
- **Visual Design:** Aligned with dashboard (`config_menu.rs`) design language — ASCII logo, fixed-size centered popups, semantic colors per step type, styled help bar, search cursors (▌), count indicators

#### Wizard Visual Design (Dashboard-Shell Layout)

The wizard reuses the dashboard's persistent layout as its canvas — logo above, a single 66-wide centered box for all step content, and a help bar below:

```
              ┌─ Lo-phi ASCII logo (Cyan bold, Magenta phi) ─┐
              │    "Feature Reduction as simple as phi"       │
              └───────────────────────────────────────────────┘
         ┌──────────────── Step Title ────────────────┐
         │                                            │
         │   (step content renders inside this box)   │
         │                                            │
         ├──── Step 3/8 ──────────── 5/12 columns ───┤
         └────────────────────────────────────────────┘
           Enter next  Backspace back  Q/Esc quit
```

- **Persistent Shell:** Single 66-wide centered box (matching dashboard's `draw_ui`); all wizard steps render inside its inner area — no per-step floating popups
- **Logo:** Lo-phi ASCII art + tagline centered above the box (9 rows, identical to dashboard)
- **Progress:** `" Step N/M "` rendered as overlay Paragraph on the box's bottom border (centered)
- **Semantic Colors:** Border/title color changes per step type — Magenta (target), Yellow (thresholds/inputs), Red (drop columns), Green (solver/weight/summary), Cyan (navigation/general)
- **Help Bar:** Borderless row below the box; Cyan keys + DarkGray descriptions, context-sensitive per step
- **Search Fields:** DarkGray bordered inner block, White text + semantic-colored cursor (▌)
- **List Selection:** `fg(Black).bg(semantic_color).bold()` (inverted background)
- **Count Indicators:** `" 3/12 columns "` at bottom-right of box border for list-based steps
- **Backspace Handling:** Per-step — in input fields, Backspace deletes characters when input is non-empty; only navigates to previous step when input is empty. Non-input steps use Backspace solely for navigation.
- **Helper Functions:** `centered_fixed_rect()` (quit overlay only), `step_color()`, `render_threshold_content()` (shared by missing/gini/correlation threshold renderers), `render_logo()`

### Interactive TUI Options (Dashboard Mode)

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
- `[F]` - Convert format (CSV/SAS7BDAT to Parquet, Parquet to CSV)
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

## Build Infrastructure

### Icon and Branding
- **`assets/icon.svg`** - Source SVG icon (gradient phi symbol, cyan-to-magenta)
- **`assets/icon.ico`** - Windows icon (16/32/48/64/128/256px multi-size)
- **`assets/icon_512.png`** - High-res PNG for README and macOS .app
- **`build.rs`** - Embeds `.ico` into Windows binary via `winres` (conditional, no-op on non-Windows)

### Release Workflow
- **`release.yml`** triggers on tag push (`v*`) or manual dispatch
- **Build matrix** (3 targets):
  - `aarch64-apple-darwin` on `macos-14` → `lophi-macos-aarch64.tar.gz`
  - `x86_64-unknown-linux-gnu` on `ubuntu-latest` → `lophi-linux-x86_64.tar.gz`
  - `x86_64-pc-windows-msvc` on `windows-latest` → `lophi-windows-x86_64.exe`
- **Windows**: Builds `.exe` with embedded icon (standalone binary)
- **Unix**: Tarballs contain the `lophi` binary
- **Release job**: Creates GitHub Release with 3 assets
- **Update-homebrew job**: Downloads macOS + Linux tarballs, computes SHA256 hashes, clones `neelsbester/homebrew-tap`, templates `Formula/lophi.rb` with version + hashes, commits and pushes

### Homebrew Tap Distribution
- **Tap repo**: `neelsbester/homebrew-tap` — install via `brew install neelsbester/tap/lophi`
- **Formula**: `Formula/lophi.rb` — precompiled binary formula with `on_macos` / `on_linux` platform detection
- **Auto-updated**: The `update-homebrew` job in `release.yml` rewrites the formula on every tagged release
- **PAT secret**: `HOMEBREW_TAP_TOKEN` (fine-grained PAT with Contents: Read & Write on `neelsbester/homebrew-tap`) must be set in lo-phi repo Actions secrets
- **Intel Mac**: Covered via Rosetta 2 running the ARM64 binary

### Scoop Bucket Distribution (Windows)
- **Bucket repo**: `neelsbester/scoop-bucket` — install via `scoop bucket add lophi https://github.com/neelsbester/scoop-bucket && scoop install lophi`
- **Manifest**: `bucket/lophi.json` — standalone `.exe` binary with `bin` alias mapping (`lophi-windows-x86_64.exe` -> `lophi`)
- **Auto-updated**: The `update-scoop` job in `release.yml` rewrites the manifest on every tagged release
- **PAT secret**: `SCOOP_BUCKET_TOKEN` (fine-grained PAT with Contents: Read & Write on `neelsbester/scoop-bucket`) must be set in lo-phi repo Actions secrets
- **Autoupdate**: Manifest includes `checkver` and `autoupdate` fields for Scoop's built-in updater

### Interactive Exit Prompt
- `main.rs` shows "Press Enter to exit..." after pipeline completion in interactive mode
- Skipped with `--no-confirm` to preserve automation compatibility
- Enables double-click-to-run UX on both Windows and macOS

## Future Enhancements (TODO)

### Correlation Analysis Improvements

1. **WoE-Encoded Correlation for Numeric Features**
   - Current: Correlation uses raw feature values
   - Proposed: Option to use WoE-encoded values for correlation
   - Benefit: Measures correlation in "predictive space" rather than raw linear relationships
   - Infrastructure exists: `find_woe_for_value()` in `iv.rs` already maps values to WoE
   - Consideration: Results become dependent on binning parameters

### Completed TUI Improvements

- **Ratatui 0.30** — upgraded from 0.29 (no breaking changes encountered in this codebase)
- **In-TUI Progress Overlay** — pipeline runs inside ratatui alternate screen with animated stage checklist; `--no-confirm` mode retains indicatif; conversion wizard path also uses TUI overlay (Loading → Converting → Saving with format/size summary)

### SAS7BDAT Parser Audit (2026-03-03)

Deep audit of the SAS7BDAT parser against readstat/pandas reference implementations. Fixes applied:

**Critical (3):**
- `is_missing_value()` now validates full 8-byte pattern (sentinel + 7 zero bytes) preventing ~11% false-positive NULLs on legitimate doubles; `isnan()` fallback catches real SAS7BDAT NaN-encoded missing values
- FormatAndLabel subheader offsets corrected to readstat spec (32-bit: 34-44, 64-bit: 46-56); single-entry-per-subheader instead of erroneous table iteration
- Compressed MIX pages no longer call `extract_rows_from_page()` (all rows are in compressed subheaders)

**High (5):**
- Encoding unified between `column.rs` and `data.rs` via `encoding_rs` (Windows-1252 0x80-0x9F now correct for column names)
- Date/time format coverage expanded from 8 to 30+ SAS formats (MONYY, E8601DA, DTDATE, TOD, etc.)
- `ColumnValue::clone()` replaced with move semantics in 3 hot loops (eliminates heap allocs for string cells)
- ColumnName/ColumnAttrs entry count uses pandas formula with column_count cap (prevents phantom entries from padding)
- `MISSING_STANDARD_PATTERN` constants corrected (removed erroneous 0xF0 byte)

**Medium (8):** Magic validation bytes 12-31 only, ZeroColumns error semantics, page_index in decompression errors, rechunk after SAS load, checked_mul for 32-bit safety, early break in column metadata collection

**Test infrastructure:** 26 integration tests with cross-validation against pandas, 34 fixture files (including tagged-na, corrupt, max_sas_date, dates_null from pandas/haven)

### Security Audit (2026-03-04)

Comprehensive security audit covering memory safety, input validation, dependency vulnerabilities, algorithmic DoS, and logic flaws. Fixes applied:

**Critical (1):**
- `bytes` crate updated from 1.11.0 to 1.11.1 to fix RUSTSEC-2026-0007 integer overflow in `BytesMut::reserve`

**High (1):**
- MIP solver timeout and gap tolerance now enforced via `set_time_limit()` and `set_mip_rel_gap()` on the HiGHS backend (`solver/model.rs`); previously `SolverConfig.timeout_seconds` was defined but never passed to the solver

**Medium (5):**
- CSV formula injection: `feature.name` in `export_reduction_report_csv()` now escaped; `escape_csv_field()` handles formula-triggering characters (`=`, `+`, `-`, `@`, `\t`, `\r`)
- `get_sas7bdat_columns()` now validates `page_size` against 256MB limit (matching `load_sas7bdat_impl()`)
- Total cell count check (`row_count * column_count > 2B`) prevents excessive pre-allocation in SAS parser
- Date/datetime/time conversions use `i32::try_from` and range validation instead of truncating `as` casts, returning `Null` for out-of-range values
- Subheader entry count uses `saturating_sub` instead of bare subtraction to prevent underflow on inconsistent metadata

**Low (2):**
- `data_start + row_stride` uses `checked_add` to prevent wraparound
- Compressed row pointer `offset`/`length` use `usize::try_from` instead of `as usize` (consistent with `process_subheader`)

### Correlation Module Audit (2026-03-04)

Audit of `src/pipeline/correlation.rs` covering statistical correctness, null handling, numerical stability, and drop logic. Fixes applied:

**Medium (2):**
- Matrix/pairwise null inconsistency: `find_correlated_pairs_auto_impl` now falls back to pairwise when any numeric column has nulls (matrix path's mean-imputation was inconsistent with pairwise deletion); matrix path uses global `sum_w` normalizer instead of per-column `sum_w_valid`
- Matrix path zero-variance check changed from exact `== 0.0` to `< f64::EPSILON` (consistent with pairwise path)

**Low (3):**
- Cramér's V bias correction guarded against `n <= 1.0` (division by zero in `(n - 1.0)`)
- Progress bar `total_pairs` now uses `float_columns.len()` (post-filter count) instead of `num_cols` (pre-filter); also fixes latent OOB bug in pair index generation
- Eta return value clarified with comment (returns `sqrt(η²)` for threshold comparability with |Pearson r| and Cramér's V)

**Test infrastructure:** 17 new tests (69 total across `test_correlation.rs` and `test_categorical_correlation.rs`): all-null columns, single-row, 2x2 hand-calculated Cramér's V reference, Eta perfect separation (== 1.0), high-cardinality boundary (100 vs 101), null handling for Cramér's V/Eta, partial metadata drop logic, tight matrix/pairwise tolerance (1e-6), large value numerical stability (1e10), null-triggered pairwise fallback verification
