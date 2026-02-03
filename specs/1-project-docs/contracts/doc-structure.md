# Contract: Documentation File Structure

**Version:** 1.0.0
**Date:** 2026-02-01
**Status:** Draft

---

## Overview

This contract defines the exact file structure, required sections, FR/NFR traceability, and dependencies for all documentation artifacts in the Lo-phi project reference documentation suite.

## Directory Structure

```
docs/
├── architecture.md          # FR-1: System architecture
├── algorithms.md            # FR-2: Statistical methods
├── user-guide.md            # FR-3: CLI/TUI reference
├── developer-guide.md       # FR-4: Developer onboarding
├── output-reference.md      # FR-5: Output file formats
├── glossary.md              # FR-8: Domain terms
├── worked-example.md        # FR-9: End-to-end example
├── STATUS.md                # Documentation status tracker (not user-facing)
└── adr/                     # FR-6: Decision records
    ├── ADR-001-polars-framework.md
    ├── ADR-002-highs-solver.md
    ├── ADR-003-cart-default-binning.md
    ├── ADR-004-woe-convention.md
    ├── ADR-005-welford-correlation.md
    ├── ADR-006-sequential-pipeline.md
    ├── ADR-007-dual-file-format.md
    └── ADR-008-ratatui-tui.md
```

---

## Document Specifications

### 1. architecture.md

**FR Traceability:**
- **FR-1:** Architecture Reference Document
- **FR-7:** Cross-Referencing & Navigation (to ADRs, algorithm guide, developer guide)

**NFR Traceability:**
- **NFR-1:** Accuracy (must match current module structure)
- **NFR-2:** Maintainability (modular structure enables updates)
- **NFR-3:** Accessibility (Markdown, GitHub-renderable, intermediate Rust level)
- **NFR-4:** Completeness (every module documented)

**Required Sections:**
1. **System Overview** (200 words)
   - Purpose statement
   - Core capabilities (missing analysis, IV/Gini, correlation)
   - Text-based architecture diagram
2. **Module Structure** (400 words)
   - CLI module (`src/cli/`) - argument parsing, TUI, conversion
   - Pipeline module (`src/pipeline/`) - loader, missing, IV, correlation, target, weights, solver
   - Report module (`src/report/`) - summary, gini export, reduction report
   - Utils module (`src/utils/`) - progress, styling
3. **Pipeline Flow** (300 words)
   - Sequential stages: Config → Load → Missing → IV → Correlation → Save
   - Data transformations at each stage
   - State management (AppState, configuration)
4. **Key Design Patterns** (200 words)
   - Error handling (`thiserror` for domain errors, `anyhow` for handlers)
   - Progress reporting strategy (indicatif)
   - Parallel processing approach (rayon for IV, correlation)
5. **Technology Stack** (100 words)
   - Polars (DataFrame operations, lazy/streaming)
   - Rayon (parallelization)
   - Ratatui/Crossterm (TUI)
   - HiGHS (solver for monotonic binning)
   - Indicatif (progress bars)
   - Zip (report bundling)

**Word Count Estimate:** 1200-1500 words

**Source Code Dependencies:**
- `src/main.rs`
- `src/pipeline/mod.rs`
- `src/cli/mod.rs`
- `src/report/mod.rs`
- `src/utils/mod.rs`
- `Cargo.toml`

**Cross-References:**
- Links to `algorithms.md` for statistical method details
- Links to `developer-guide.md` for contribution patterns
- Links to ADRs: ADR-001 (Polars), ADR-006 (sequential pipeline), ADR-008 (Ratatui)
- Links to `glossary.md` for term definitions

**Validation Criteria:**
- All modules in `src/` are documented
- Pipeline flow matches execution order in `src/main.rs`
- Technology choices match `Cargo.toml` dependencies
- All linked ADRs exist

---

### 2. algorithms.md

**FR Traceability:**
- **FR-2:** Algorithm & Statistical Methods Guide
- **FR-7:** Cross-Referencing & Navigation

**NFR Traceability:**
- **NFR-1:** Accuracy (formulas must match implementation exactly)
- **NFR-3:** Accessibility (assumes basic statistics background)
- **NFR-4:** Completeness (all formulas, constants, edge cases)

**Required Sections:**
1. **Overview** (100 words)
   - Three-stage feature selection approach
2. **Weight of Evidence (WoE) Binning** (800 words)
   - Mathematical definition: `WoE = ln((Bad/Total_Bad) / (Good/Total_Good))`
   - With Laplace smoothing: `WoE = ln(((events + 0.5)/(total_events + 0.5)) / ((non_events + 0.5)/(total_non_events + 0.5)))`
   - Convention: WoE > 0 indicates higher risk (more events/defaults); WoE < 0 indicates lower risk
   - Binning strategies:
     - CART: Decision tree-based splits
     - Quantile: Equal-frequency binning
   - Pre-binning process (`DEFAULT_PREBINS = 20`)
   - Bin merging rules (`MIN_BIN_SAMPLES = 5`)
   - Monotonicity constraints (ascending, descending, peak, valley, auto)
   - Edge cases: `events == 0`, `non_events == 0`
   - Laplace smoothing (`SMOOTHING = 0.5`)
   - Categorical feature handling (frequency-based binning, missing category)
3. **Information Value (IV) Calculation** (200 words)
   - Formula: `IV = sum((Good_i/Total_Good - Bad_i/Total_Bad) * WoE_i)`
   - Interpretation thresholds:
     - IV < 0.02: Not predictive
     - 0.02 ≤ IV < 0.1: Weak
     - 0.1 ≤ IV < 0.3: Medium
     - 0.3 ≤ IV < 0.5: Strong
     - IV ≥ 0.5: Very strong (suspect)
4. **Gini Coefficient** (150 words)
   - Formula: `Gini = 2 * AUC - 1`
   - Relationship to IV
   - Calculation from WoE bins
5. **Pearson Correlation** (250 words)
   - Formula: `r = cov(X,Y) / (σ_X * σ_Y)`
   - Welford algorithm for numerical stability
   - Single-pass computation
   - Parallel strategy (rayon)
6. **Missing Value Analysis** (100 words)
   - Null ratio formula: `null_ratio = null_count / total_count`
   - Threshold interpretation
7. **Solver-Based Binning Optimization** (300 words)
   - MIP model formulation
   - Decision variables (binary indicators for splits)
   - Objective: Maximize IV
   - Monotonicity constraints (linear inequalities)
   - Solver parameters:
     - Timeout (default: 30s)
     - Gap tolerance (default: 0.01)
   - Trend detection heuristics (auto mode)
8. **Constants Reference** (100 words)
   - `DEFAULT_PREBINS = 20`
   - `MIN_BIN_SAMPLES = 5`
   - `SMOOTHING = 0.5`
   - Default thresholds: missing=0.30, gini=0.05, correlation=0.40

**Word Count Estimate:** 2000-2500 words

**Source Code Dependencies:**
- `src/pipeline/iv.rs` (primary: ~2600 lines)
- `src/pipeline/correlation.rs`
- `src/pipeline/missing.rs`
- `src/pipeline/solver/model.rs`
- `src/pipeline/solver/monotonicity.rs`
- `src/pipeline/solver/precompute.rs`

**Cross-References:**
- Links to `glossary.md` for all technical terms
- Links to `architecture.md` for module context
- Referenced by `output-reference.md` for field interpretation

**Validation Criteria:**
- All formulas verified against source code
- All constants match values in source
- Edge case handling documented for all zero-count scenarios
- Smoothing formula matches `SMOOTHING` constant usage

---

### 3. user-guide.md

**FR Traceability:**
- **FR-3:** User & Configuration Guide
- **FR-7:** Cross-Referencing & Navigation

**NFR Traceability:**
- **NFR-1:** Accuracy (CLI args, defaults, ranges match source)
- **NFR-3:** Accessibility (assumes no Rust knowledge)
- **NFR-4:** Completeness (all CLI options, all TUI shortcuts)

**Required Sections:**
1. **Installation & Quick Start** (150 words)
   - Prerequisites (Rust toolchain)
   - Build command: `cargo build --release`
   - Basic usage: `lophi <INPUT_FILE>`
2. **CLI Mode Reference** (600 words)
   - Table of all CLI arguments:
     - `--input` (required)
     - `--target` (required)
     - `--missing-threshold` (default: 0.30, range: 0.0-1.0)
     - `--gini-threshold` (default: 0.05, range: 0.0-1.0)
     - `--correlation-threshold` (default: 0.40, range: 0.0-1.0)
     - `--binning-strategy` (cart, quantile, default: cart)
     - `--gini-bins` (default: 10)
     - `--prebins` (default: 20)
     - `--cart-min-bin-pct` (default: 5.0)
     - `--min-category-samples` (default: 5)
     - `--use-solver` (default: true)
     - `--solver-timeout` (default: 30)
     - `--solver-gap` (default: 0.01)
     - `--trend` (none, ascending, descending, peak, valley, auto)
     - `--weight-column` (optional)
     - `--drop` (optional, repeatable)
     - `--schema-inference-rows` (default: 10000)
   - Example commands
3. **Interactive TUI Mode** (400 words)
   - Launching: `lophi` (no arguments)
   - Three-column layout:
     - THRESHOLDS (Missing, Gini, Correlation)
     - SOLVER (Use Solver toggle, Trend/Monotonicity)
     - DATA (Drop columns, Weight column, Schema inference)
   - Keyboard shortcuts:
     - `[Enter]` - Run with current settings
     - `[T]` - Select target column
     - `[F]` - Convert CSV to Parquet
     - `[D]` - Select columns to drop
     - `[C]` - Edit thresholds (chained flow)
     - `[S]` - Edit solver options
     - `[W]` - Select weight column
     - `[A]` - Advanced options
     - `[Q]` - Quit
4. **Configuration Parameters** (250 words)
   - Thresholds: behavioral effects
   - Solver options: when to use, monotonicity guidance
   - Weight column: purpose and constraints
   - Schema inference: performance vs accuracy trade-off
5. **CSV to Parquet Conversion** (100 words)
   - Using `[F]` in TUI
   - Fast in-memory mode
   - Output location
6. **Common Workflows** (200 words)
   - Quick analysis with defaults
   - Custom threshold tuning
   - Solver-based monotonic binning
   - Weighted analysis

**Word Count Estimate:** 1500-1800 words

**Source Code Dependencies:**
- `src/cli/args.rs`
- `src/cli/config_menu.rs`
- `src/cli/convert.rs`
- `src/main.rs` (default values)

**Cross-References:**
- Links to `glossary.md` for technical terms
- Referenced by `developer-guide.md` for configuration structure

**Validation Criteria:**
- Every field in `Args` struct is documented
- All keyboard shortcuts from `config_menu.rs` are documented
- Default values match source code
- Valid ranges match validation logic
- TUI layout description verified against actual rendering

---

### 4. developer-guide.md

**FR Traceability:**
- **FR-4:** Developer Onboarding Guide
- **FR-7:** Cross-Referencing & Navigation

**NFR Traceability:**
- **NFR-1:** Accuracy (setup instructions, test commands)
- **NFR-2:** Maintainability (enables future contributions)
- **NFR-3:** Accessibility (assumes intermediate Rust knowledge)
- **NFR-4:** Completeness (all dev workflows covered)

**Required Sections:**
1. **Development Setup** (200 words)
   - Prerequisites: Rust 1.70+ (check `Cargo.toml` `rust-version`)
   - Clone: `git clone <repo>`
   - Build: `cargo build`
   - Run tests: `cargo test --all-features`
   - IDE: VS Code + rust-analyzer, IntelliJ IDEA + Rust plugin
2. **Project Structure** (300 words)
   - Directory layout (src/, tests/, benches/, docs/)
   - Module responsibilities (matches architecture.md)
   - File organization conventions
3. **Code Conventions** (400 words)
   - Formatting: `cargo fmt` (enforced in CI)
   - Linting: `cargo clippy --all-targets --all-features -- -D warnings`
   - Error handling:
     - Domain errors: `thiserror` (e.g., `PipelineError`)
     - Handler errors: `anyhow::Result` with context
   - Naming: snake_case, descriptive, avoid abbreviations
   - Comments: doc comments for public API, inline for complex logic
4. **Testing** (500 words)
   - Test commands:
     - All tests: `cargo test --all-features`
     - Unit only: `cargo test --lib --all-features`
     - Integration only: `cargo test --test '*' --all-features`
     - Single test: `cargo test --all-features <NAME> -- --nocapture`
   - Test structure:
     - `tests/common/mod.rs` - fixtures (`create_test_dataframe()`, temp files, assertions)
     - Integration tests: `test_pipeline.rs`, `test_missing.rs`, etc.
   - Fixtures and helpers:
     - `create_test_dataframe()` - synthetic data
     - `create_temp_csv()`, `create_temp_parquet()` - temp file helpers
     - Assertion helpers
   - Writing new tests:
     - Use fixtures for consistency
     - Test edge cases (nulls, empty data, extreme values)
     - Coverage expectations (>80% line coverage)
5. **Benchmarking** (200 words)
   - Run benchmarks: `cargo bench`
   - Benchmark structure: `benches/binning_benchmark.rs`
   - Interpreting results (criterion output)
6. **Contributing** (300 words)
   - Git workflow: feature branches, PRs to main
   - Commit message format: `fix:`, `feature:`, `docs:`, `chore:`
   - Pull request process: tests pass, clippy clean, description
   - Code review expectations
7. **Common Development Tasks** (400 words)
   - Adding a new pipeline stage:
     - Create module in `src/pipeline/`
     - Implement analysis function
     - Integrate into `main.rs` pipeline
     - Add tests
     - Update documentation
   - Adding a new CLI option:
     - Add field to `Args` struct in `args.rs`
     - Update TUI if applicable (`config_menu.rs`)
     - Document in `user-guide.md`
   - Adding a new TUI shortcut:
     - Handle key in `config_menu.rs` match block
     - Update layout if needed
     - Document in `user-guide.md`
   - Updating output formats:
     - Modify serialization in `report/` module
     - Update `output-reference.md`
     - Add tests verifying schema

**Word Count Estimate:** 1800-2200 words

**Source Code Dependencies:**
- `Cargo.toml`
- `Makefile` (if exists)
- `tests/common/mod.rs`
- `tests/test_*.rs`
- `benches/binning_benchmark.rs`
- `.github/workflows/` (CI config)

**Cross-References:**
- Links to `architecture.md` for system overview
- Links to `user-guide.md` for configuration
- Links to `algorithms.md` for statistical background

**Validation Criteria:**
- Setup instructions work on clean Ubuntu/macOS/Windows
- All test commands execute successfully
- Code conventions match actual codebase (verify with clippy/fmt)
- Common task examples are accurate

---

### 5. output-reference.md

**FR Traceability:**
- **FR-5:** Output & Report Reference
- **FR-7:** Cross-Referencing & Navigation

**NFR Traceability:**
- **NFR-1:** Accuracy (schemas match serialization)
- **NFR-3:** Accessibility (no code knowledge required for interpretation)
- **NFR-4:** Completeness (all output files, all fields)

**Required Sections:**
1. **Output File Overview** (100 words)
   - File naming: `{input}_reduced.{ext}`, `{input}_reduction_report.zip`
   - Output directory (same as input)
2. **Reduced Dataset** (150 words)
   - Format: CSV or Parquet (matches input)
   - Schema: subset of input columns (features not dropped)
   - Row preservation: all input rows retained
3. **Reduction Report ZIP Bundle** (100 words)
   - Contents: `*_gini_analysis.json`, `*_reduction_report.json`, `*_reduction_report.csv`
   - Extraction: standard zip tools
4. **Gini Analysis JSON** (400 words)
   - Top-level: array of `IvAnalysis` objects
   - `IvAnalysis` fields:
     - `feature_name` (string)
     - `feature_type` ("numeric" | "categorical")
     - `iv` (f64)
     - `gini` (f64)
     - `bins` (array, for numeric features)
     - `categories` (array, for categorical features)
   - `WoeBin` fields (numeric):
     - `lower_bound` (f64 | null for -∞)
     - `upper_bound` (f64 | null for +∞)
     - `woe` (f64)
     - `iv_contribution` (f64)
     - `count` (u64)
     - `events` (u64)
     - `non_events` (u64)
     - `event_rate` (f64)
   - `CategoricalWoeBin` fields:
     - `category` (string)
     - `woe`, `iv_contribution`, `count`, `events`, `non_events`, `event_rate` (same as numeric)
   - Example JSON snippet
5. **Reduction Report JSON** (300 words)
   - Schema overview
   - Per-feature metadata:
     - `feature` (string)
     - `null_ratio` (f64 | null)
     - `iv` (f64 | null)
     - `gini` (f64 | null)
     - `correlated_features` (array of objects with `feature` and `correlation` fields)
     - `drop_reason` (string | null)
     - `retained` (boolean)
   - Analysis summary fields
   - Example snippet
6. **Reduction Report CSV** (200 words)
   - Row-per-feature format
   - Columns:
     - `feature`, `null_ratio`, `iv`, `gini`, `correlated_features`, `drop_reason`, `retained`
   - Correlated features format: pipe-separated `feature: correlation | feature2: correlation`
   - Example row
7. **Interpreting Results** (250 words)
   - IV thresholds: weak/medium/strong/suspect
   - WoE interpretation: positive = higher good rate
   - Correlation: high values indicate redundancy
   - Missing value ratios: thresholds guide

**Word Count Estimate:** 1200-1500 words

**Source Code Dependencies:**
- `src/report/gini_export.rs`
- `src/report/reduction_report.rs`
- `src/report/summary.rs`
- `src/pipeline/iv.rs` (struct definitions)

**Cross-References:**
- Links to `algorithms.md` for IV/WoE/Gini formulas
- Links to `glossary.md` for term definitions

**Validation Criteria:**
- All output file types documented
- JSON schemas match Rust struct serialization (verify with sample output)
- CSV columns match actual output
- Field descriptions accurate
- Example snippets are real Lo-phi output

---

### 6. glossary.md

**FR Traceability:**
- **FR-8:** Glossary of Terms

**NFR Traceability:**
- **NFR-3:** Accessibility (defines all domain terms)
- **NFR-4:** Completeness (all terms used in documentation)

**Required Sections:**
1. **Introduction** (50 words)
   - Purpose: define domain-specific and technical terms
2. **Terms** (750-1150 words)
   - Alphabetical list
   - Each entry:
     - **Term**
     - Definition (1-2 sentences, concise)
     - Context (where used in Lo-phi)
     - Related terms
     - Formula (if applicable, LaTeX notation)

**Terms to Include:**
- Bad Rate
- Binning
- CART (Classification and Regression Trees)
- Cramér's V (future feature, mark as planned)
- Event Rate
- Feature Reduction
- Gap Tolerance
- Gini Coefficient
- Good Rate
- Information Value (IV)
- Laplace Smoothing
- MIP (Mixed-Integer Programming)
- Monotonicity Constraint
- Null Ratio
- Pearson Correlation
- Population Splitting
- Prebinning
- Quantile Binning
- Schema Inference
- Solver
- Weighted Analysis
- Welford Algorithm
- Weight of Evidence (WoE)

**Word Count Estimate:** 800-1200 words

**Source Code Dependencies:**
- All modules (for term extraction)
- Constants in `src/pipeline/iv.rs`

**Cross-References:**
- Referenced by all other documents
- No outbound links (foundational reference)

**Validation Criteria:**
- All terms used in other documentation are defined
- Definitions are accurate and non-circular
- Formulas match `algorithms.md`
- No unused terms

---

### 7. worked-example.md

**FR Traceability:**
- **FR-9:** End-to-End Worked Example

**NFR Traceability:**
- **NFR-1:** Accuracy (uses real Lo-phi output)
- **NFR-3:** Accessibility (step-by-step walkthrough)

**Required Sections:**
1. **Introduction** (100 words)
   - Purpose: demonstrate full pipeline on synthetic dataset
2. **Example Dataset** (200 words)
   - Description: 20 rows, 3 numeric features (1 high-IV, 1 low-IV, 1 high-missing), 2 categorical features (1 high-IV, 1 medium-IV), 1 binary target
   - CSV snippet (first 10 rows)
3. **Configuration** (150 words)
   - Selected parameters:
     - Missing threshold: 0.30
     - Gini threshold: 0.05
     - Correlation threshold: 0.40
     - Binning strategy: CART
     - Use solver: true
     - Trend: auto
   - Rationale for choices
4. **Pipeline Execution** (150 words)
   - Command line: `lophi data.csv --target default --use-solver --trend auto`
   - Or TUI selection steps
5. **Step-by-Step Analysis** (700 words)
   - **Missing Value Analysis:**
     - Results table (feature, null_ratio, pass/fail)
     - Example: "age: 0.02 (pass), income: 0.35 (fail - dropped)"
   - **IV/Gini Analysis:**
     - Results table (feature, IV, Gini, pass/fail)
     - WoE bins for one feature (annotated):
       - Bin 1: [0, 30), WoE=-0.5, IV_contrib=0.02, events=10, non_events=40
       - Bin 2: [30, 50), WoE=0.2, IV_contrib=0.01, events=25, non_events=25
       - Bin 3: [50, ∞), WoE=0.8, IV_contrib=0.05, events=40, non_events=10
       - Total IV=0.08
     - Example: "credit_score: IV=0.08 (pass), debt_ratio: IV=0.02 (fail - dropped)"
   - **Correlation Analysis:**
     - Correlation matrix snippet
     - Example: "age and credit_score: r=0.85 (high - drop credit_score based on lower IV)"
6. **Output Files** (400 words)
   - **Reduced dataset:**
     - Features retained: age, employment_status, default
     - Row count: 500 (unchanged)
   - **Gini analysis JSON:**
     - Annotated snippet for one feature
   - **Reduction report CSV:**
     - Annotated row showing all fields
   - **Reduction report JSON:**
     - Key sections explained
7. **Interpretation Summary** (200 words)
   - Why each feature was kept/dropped:
     - income: dropped (high missing ratio)
     - debt_ratio: dropped (low IV)
     - credit_score: dropped (high correlation with age, lower IV)
     - age: kept (passes all thresholds)
     - employment_status: kept (categorical, sufficient IV)
   - How to read WoE: positive WoE means higher good rate (less likely to default)
   - Correlation impact: redundant features removed

**Word Count Estimate:** 1500-2000 words

**Source Code Dependencies:**
- None (uses output of running Lo-phi on synthetic data)

**Cross-References:**
- Links to `algorithms.md` for formula context
- Links to `output-reference.md` for field definitions
- Links to `user-guide.md` for configuration parameters
- Links to `glossary.md` for term lookups

**Validation Criteria:**
- Example is reproducible (synthetic data script or CSV provided)
- All output snippets are real Lo-phi output
- Annotations correctly explain values
- Covers all three pipeline stages (missing, IV, correlation)
- Demonstrates at least one kept and one dropped feature per stage

---

### 8. ADR Collection (docs/adr/)

**FR Traceability:**
- **FR-6:** Architectural Decision Records

**NFR Traceability:**
- **NFR-1:** Accuracy (decisions match current codebase)
- **NFR-2:** Maintainability (enables future decision tracking)
- **NFR-3:** Accessibility (lightweight template, clear rationale)

**ADR Files:**

#### ADR-001-polars-framework.md

**Context:** Need for high-performance DataFrame operations with CSV/Parquet support.

**Decision:** Use Polars as the DataFrame framework.

**Alternatives Considered:**
- pandas (Python, not Rust-native)
- DataFusion (more complex, query-engine focus)
- ndarray (too low-level, no built-in CSV/Parquet)

**Consequences:**
- ✅ Pure Rust, no Python dependency
- ✅ Lazy evaluation and streaming for large datasets
- ✅ Native CSV/Parquet support
- ✅ Excellent performance
- ❌ Ecosystem smaller than pandas
- ❌ API changes in early versions

**Word Count:** 250-350 words

---

#### ADR-002-highs-solver.md

**Context:** Need for MIP solver to optimize WoE binning with monotonicity constraints.

**Decision:** Use HiGHS solver via `good_lp` crate with `highs` feature flag.

**Alternatives Considered:**
- CBC (Coin-OR solver, C++ dependency, slower)
- GLPK (GPL license, licensing concerns)
- Custom solver (too complex for this use case)

**Consequences:**
- ✅ MIT license (permissive)
- ✅ Fast performance
- ✅ Rust bindings available
- ✅ Active development
- ❌ External dependency (C++ library)
- ❌ Build complexity on some platforms

**Word Count:** 250-350 words

---

#### ADR-003-cart-default-binning.md

**Context:** Need to choose default binning strategy between CART and Quantile.

**Decision:** Use CART as the default binning strategy.

**Alternatives Considered:**
- Quantile (simpler, equal-frequency bins)
- Fixed-width (too simplistic)

**Consequences:**
- ✅ Better class separation (decision tree splits maximize purity)
- ✅ More intuitive for users with ML background
- ✅ Works well with solver for monotonicity
- ❌ More complex implementation
- ❌ Slower than quantile for large datasets
- Note: Quantile still available via CLI option

**Word Count:** 200-300 words

---

#### ADR-004-woe-convention.md

**Context:** Multiple WoE sign conventions exist in literature: `ln(Bad/Good)` vs `ln(Good/Bad)`.

**Decision:** Use `ln(Bad/Good)` convention (i.e., `ln(%bad/%good)`) where positive WoE indicates higher risk.

**Alternatives Considered:**
- `ln(Good/Bad)` (positive WoE = lower risk, less common in credit scoring)

**Consequences:**
- ✅ Positive WoE indicates higher risk (intuitive for credit scoring where higher WoE = higher default rate)
- ✅ Aligns with common credit scoring industry practice
- ✅ Monotonicity interpretation clearer (ascending WoE = ascending risk)
- ❌ Some academic papers use opposite sign convention
- Note: Documented clearly in algorithm guide to avoid confusion

**Word Count:** 200-300 words

---

#### ADR-005-welford-correlation.md

**Context:** Need numerically stable method for Pearson correlation calculation.

**Decision:** Use Welford's online algorithm for correlation computation.

**Alternatives Considered:**
- Two-pass algorithm (read data twice)
- Naive formula (numerically unstable for large values)

**Consequences:**
- ✅ Single-pass (memory efficient)
- ✅ Numerically stable (avoids catastrophic cancellation)
- ✅ Parallelizable (combine accumulators)
- ❌ More complex implementation than naive formula
- Note: Rayon used for parallel computation across feature pairs

**Word Count:** 200-300 words

---

#### ADR-006-sequential-pipeline.md

**Context:** Need to define pipeline stage ordering (missing → IV → correlation).

**Decision:** Use sequential pipeline with strict ordering: Missing → IV/Gini → Correlation.

**Alternatives Considered:**
- Parallel stages (all analyses run independently)
- Configurable ordering

**Consequences:**
- ✅ Predictable behavior (no order-dependent surprises)
- ✅ Easier to reason about (linear flow)
- ✅ Avoids analyzing dropped features (efficiency)
- ❌ Cannot run stages in parallel (but each stage is parallelized internally)
- ❌ Order may affect final results (e.g., correlation calculated after IV drops)
- Note: Order chosen based on computational cost (missing cheapest, correlation most expensive)

**Word Count:** 250-350 words

---

#### ADR-007-dual-file-format.md

**Context:** Need to decide on supported input/output file formats.

**Decision:** Support both CSV and Parquet for input and output.

**Alternatives Considered:**
- CSV only (simpler, universally compatible)
- Parquet only (faster, smaller, but less accessible)

**Consequences:**
- ✅ Broader compatibility (users can choose based on needs)
- ✅ Parquet performance for large datasets
- ✅ CSV accessibility for small datasets and debugging
- ❌ More code complexity (two parsers)
- ❌ Schema inference differences (CSV requires more heuristics)
- Note: Added CSV-to-Parquet conversion utility in TUI

**Word Count:** 200-300 words

---

#### ADR-008-ratatui-tui.md

**Context:** Need interactive configuration without building a GUI.

**Decision:** Use Ratatui for terminal-based user interface.

**Alternatives Considered:**
- CLI-only (less user-friendly for exploration)
- Web UI (too complex, requires server)
- egui (requires GUI, not terminal-friendly)

**Consequences:**
- ✅ Terminal-based (consistent with CLI workflow)
- ✅ Cross-platform (works in SSH, containers)
- ✅ Rust-native (no external dependencies)
- ✅ Active development and good documentation
- ❌ Learning curve for TUI development
- ❌ Limited to terminal capabilities (no rich graphics)
- Note: Three-column layout provides clear visual organization

**Word Count:** 200-300 words

---

**Total ADR Word Count Estimate:** 1600-3200 words

**Source Code Dependencies:**
- Each ADR references specific modules or `Cargo.toml` entries
- ADR-001: `Cargo.toml` (polars dependency)
- ADR-002: `Cargo.toml` (highs dependency), `src/pipeline/solver/`
- ADR-003: `src/pipeline/iv.rs` (CART implementation)
- ADR-004: `src/pipeline/iv.rs` (WoE calculation)
- ADR-005: `src/pipeline/correlation.rs` (Welford algorithm)
- ADR-006: `src/main.rs` (pipeline orchestration)
- ADR-007: `src/pipeline/loader.rs` (CSV/Parquet loading)
- ADR-008: `src/cli/config_menu.rs` (TUI implementation)

**Cross-References:**
- All ADRs referenced by `architecture.md`
- Individual ADRs may link to `algorithms.md` for technical details

**Validation Criteria:**
- Each ADR follows standard template (see `adr-template.md`)
- Context accurately describes problem
- Alternatives are documented (at least 2 alternatives per ADR)
- Consequences include both positives (✅) and negatives (❌)
- Decision aligns with current codebase

---

### 9. STATUS.md (Internal Tracking)

**Purpose:** Track documentation state transitions (Draft → Review → Published).

**Not User-Facing:** This file is for maintainer use only.

**Structure:**
```markdown
# Documentation Status

Last Updated: 2026-02-01

| Document | Status | Last Updated | Verified Against Commit | Validator |
|----------|--------|--------------|-------------------------|-----------|
| architecture.md | Draft | 2026-02-01 | - | - |
| algorithms.md | Draft | 2026-02-01 | - | - |
| user-guide.md | Draft | 2026-02-01 | - | - |
| developer-guide.md | Draft | 2026-02-01 | - | - |
| output-reference.md | Draft | 2026-02-01 | - | - |
| glossary.md | Draft | 2026-02-01 | - | - |
| worked-example.md | Draft | 2026-02-01 | - | - |
| adr/ADR-001-polars-framework.md | Draft | 2026-02-01 | - | - |
| adr/ADR-002-highs-solver.md | Draft | 2026-02-01 | - | - |
| adr/ADR-003-cart-default-binning.md | Draft | 2026-02-01 | - | - |
| adr/ADR-004-woe-convention.md | Draft | 2026-02-01 | - | - |
| adr/ADR-005-welford-correlation.md | Draft | 2026-02-01 | - | - |
| adr/ADR-006-sequential-pipeline.md | Draft | 2026-02-01 | - | - |
| adr/ADR-007-dual-file-format.md | Draft | 2026-02-01 | - | - |
| adr/ADR-008-ratatui-tui.md | Draft | 2026-02-01 | - | - |
```

**Word Count:** N/A (tracking table only)

---

## Summary Statistics

**Total Documents:** 15 files
- 7 guide documents
- 8 ADR documents
- 1 status tracker (internal)

**Total Estimated Word Count:** 11,300-15,900 words
- Guides: 9,700-12,700 words
- ADRs: 1,600-3,200 words

**Implementation Effort Ranking (by word count):**
1. algorithms.md (2000-2500 words) - Most complex, formulas
2. developer-guide.md (1800-2200 words)
3. worked-example.md (1500-2000 words)
4. user-guide.md (1500-1800 words)
5. architecture.md (1200-1500 words)
6. output-reference.md (1200-1500 words)
7. glossary.md (800-1200 words)
8. ADRs (8 × 200-400 words each)

**Dependencies:**
- **No dependencies:** glossary.md (start here)
- **Depends on glossary:** architecture.md, user-guide.md
- **Depends on architecture:** algorithms.md, developer-guide.md
- **Depends on algorithms:** output-reference.md
- **Depends on all:** worked-example.md
- **Independent:** ADRs (minimal dependencies, can be parallelized)

---

## Validation Checklist

Before marking a document as "Published":

- [ ] All required sections present
- [ ] Word count within estimated range
- [ ] All source code references resolve to actual files
- [ ] All cross-references resolve to existing documents/sections
- [ ] All formulas verified against source code
- [ ] All constants match source code values
- [ ] All CLI arguments match `args.rs`
- [ ] All TUI shortcuts match `config_menu.rs`
- [ ] All output fields match serialization structs
- [ ] Markdown renders correctly on GitHub
- [ ] LaTeX formulas render correctly
- [ ] Code blocks have language tags
- [ ] No placeholder text ("TBD", "TODO")
- [ ] No typos or grammatical errors
- [ ] Glossary terms are defined
- [ ] FR/NFR traceability complete

---

## Maintenance Protocol

When source code changes:

1. **Identify impacted documents:**
   - New module → architecture.md, developer-guide.md
   - New CLI option → user-guide.md
   - Formula change → algorithms.md, output-reference.md
   - Output format change → output-reference.md
   - New dependency → architecture.md, relevant ADR

2. **Update document:**
   - Make changes
   - Re-verify against source
   - Update "Last Updated" in STATUS.md
   - Update "Verified Against Commit" in STATUS.md

3. **Cross-reference check:**
   - Verify all links still resolve
   - Update worked-example.md if behavior changed

4. **Mark as "Review" → "Published" after verification**
