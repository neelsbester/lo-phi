# Research Document: Comprehensive Project Reference Documentation

**Spec ID**: 1-project-docs
**Research Date**: 2026-02-01
**Status**: Complete

This document consolidates research findings for creating comprehensive reference documentation for the Lo-phi feature reduction tool. Each topic includes the decision made, rationale with supporting evidence, and alternatives considered.

---

## 1. Documentation Structure

### Decision
Use `docs/` as the root documentation directory with a `docs/adr/` subdirectory for Architecture Decision Records.

### Rationale
- **Spec Requirement**: Clarification session 2026-02-01 explicitly specified this structure
- **Minimal Nesting**: Flat hierarchy improves discoverability
- **GitHub Convention**: Standard pattern recognized by GitHub Pages and documentation tooling
- **Existing Pattern**: Project already uses root-level `CLAUDE.md` for agent instructions, maintaining consistency with Markdown-based documentation approach

### Alternatives Considered
1. **Nested Structure** (`docs/guides/`, `docs/reference/`, `docs/api/`)
   - Rejected: Adds complexity for a single-binary CLI tool
   - Would fragment related content across multiple directories
2. **Single File Approach** (expand `CLAUDE.md`)
   - Rejected: Would create unmaintainable 5000+ line file
   - Poor navigation experience for users
3. **mdBook Structure** (`src/` with `SUMMARY.md`)
   - Rejected: Requires external tooling to view documentation
   - Conflicts with NFR-3 (GitHub-native rendering)

---

## 2. Document Formats

### Decision
Use standard Markdown with LaTeX-compatible notation (`$$...$$` blocks) for mathematical formulas.

### Rationale
- **GitHub Rendering**: GitHub natively renders LaTeX math blocks without external dependencies (satisfies NFR-3)
- **Existing Pattern**: Project already uses Markdown for `CLAUDE.md` and `README.md`
- **Formula Complexity**: Mathematical notation is essential for documenting WoE/IV/Gini/Correlation algorithms
- **Toolchain Simplicity**: No build step required—documentation is readable in any Markdown viewer

### Alternatives Considered
1. **mdBook** (Rust documentation framework)
   - Rejected: Requires `mdbook` installation and build step
   - Violates NFR-3 (GitHub-native rendering)
   - Overkill for 10-document project
2. **Docusaurus** (React-based documentation)
   - Rejected: Adds Node.js dependency to Rust project
   - Requires deployment infrastructure
3. **AsciiDoc** (alternative markup language)
   - Rejected: Less familiar to Rust developers
   - GitHub rendering support is inferior to Markdown

---

## 3. Algorithm Documentation Approach

### Decision
Document all formulas using LaTeX math blocks, include edge-case handling tables, and cross-reference to source code with `file:line` format.

### Rationale
- **Codebase Complexity**: Statistical algorithms span 2,585 lines across 4 modules:
  - `src/pipeline/iv.rs`: 2099 lines (WoE/IV calculation with CART and Quantile binning)
  - `src/pipeline/correlation.rs`: 486 lines (Welford algorithm, matrix-based correlation)
  - `src/pipeline/target.rs`: Implementation of binary target detection and encoding
  - `src/pipeline/missing.rs`: Null ratio calculation
- **Formula Inventory**: 5 core mathematical formulas identified:
  ```
  WoE = ln((events+0.5)/(total_events+0.5) / (non_events+0.5)/(total_non_events+0.5))
  IV = Σ(dist_events - dist_non_events) × WoE
  Gini Impurity = 2 × p × (1-p)
  Pearson Correlation (Welford single-pass algorithm)
  Gini Coefficient = 2 × AUC - 1 (Mann-Whitney U weighted)
  ```
- **Edge Cases Found**: Smoothing for zero counts, missing value handling, monotonicity constraints, minimum bin sample requirements
- **Traceability**: FR-7 requires source code references; `file:line` format enables direct navigation

### Alternatives Considered
1. **Pseudocode Only** (no LaTeX formulas)
   - Rejected: Loses mathematical precision
   - Makes it difficult to verify implementation correctness
2. **Reference to External Papers Only** (link to academic sources)
   - Rejected: External links break over time
   - Doesn't document project-specific adaptations (e.g., smoothing constants)
3. **Inline Code Snippets** (show Rust implementation)
   - Rejected as sole approach: Code alone doesn't explain mathematical intent
   - Will be used in combination with formulas for completeness

---

## 4. ADR Candidates Identified

### Decision
Document 8 Architecture Decision Records using lightweight "Title / Context / Decision / Consequences" format.

### Rationale
- **Spec Requirement**: FR-4 requires ≥5 ADRs; 8 identified provides buffer and comprehensive coverage
- **Codebase Evidence**: Each ADR has clear implementation evidence:
  1. **Polars as Data Processing Framework** (`Cargo.toml`: polars v0.45, lazy/streaming features)
  2. **HiGHS Solver via good_lp** (`src/pipeline/iv.rs:463-549`: optimal binning implementation)
  3. **CART as Default Binning Strategy** (`src/pipeline/iv.rs:166-237`: decision tree splits)
  4. **WoE = ln(%bad/%good) Convention** (`src/pipeline/iv.rs:1146-1158`: credit scoring standard)
  5. **Welford Algorithm + faer Matrix** (`src/pipeline/correlation.rs:166-286`: single-pass correlation)
  6. **Sequential Three-Stage Pipeline** (`src/main.rs:77-200`: Missing → Gini → Correlation flow)
  7. **Dual File Format Support** (`src/pipeline/loader.rs`, `src/cli/convert.rs`: CSV + Parquet)
  8. **Ratatui TUI with Three-Column Layout** (`src/cli/config_menu.rs:412-561`: interactive menu)
- **Commit History Support**: Each decision has traceable commit history in git log

### ADR List
| ADR # | Title | Primary File | Line Range |
|-------|-------|--------------|------------|
| 001 | Polars Data Processing Framework | `Cargo.toml` | 15-20 |
| 002 | HiGHS Solver for Optimal Binning | `src/pipeline/iv.rs` | 463-549 |
| 003 | CART Default Binning Strategy | `src/pipeline/iv.rs` | 166-237 |
| 004 | Credit Scoring WoE Convention | `src/pipeline/iv.rs` | 1146-1158 |
| 005 | Welford Correlation Algorithm | `src/pipeline/correlation.rs` | 166-286 |
| 006 | Sequential Pipeline Architecture | `src/main.rs` | 77-200 |
| 007 | Dual Format Support (CSV/Parquet) | `src/pipeline/loader.rs` | 1-150 |
| 008 | Ratatui Interactive TUI | `src/cli/config_menu.rs` | 412-561 |

### Alternatives Considered
1. **Full MADR Format** (Markdown Any Decision Records with Status/Consequences/Pros/Cons sections)
   - Rejected: Too heavyweight for this project size
   - Would create 4-5 pages per ADR (40+ pages total)
2. **Y-Statements Format** ("In the context of X, facing Y, we decided Z to achieve A, accepting B")
   - Rejected: Less readable than structured sections
   - Harder to scan for specific information
3. **Chronological Decision Log** (single timeline document)
   - Rejected: Poor navigability
   - Harder to find specific decision topics

---

## 5. Output File Schemas

### Decision
Document all 5 output file types with field-level schemas and example snippets.

### Rationale
- **Complete Output Inventory**:
  1. `{input}_reduced.{csv|parquet}` - Reduced dataset with dropped features removed
  2. `{input}_reduction_report.json` - Comprehensive JSON report (maps to `ReductionReport` struct)
  3. `{input}_gini_analysis.json` - Detailed WoE/IV per feature (maps to `IvAnalysis` struct)
  4. `{input}_reduction_report.csv` - Human-readable CSV summary (10 columns including pipe-separated correlations)
  5. `{input}_reduction_report.zip` - Bundled archive containing reports #2-4
- **Source Mapping**:
  - JSON schemas map to Rust structs in `src/report/reduction_report.rs` (lines 8-42)
  - CSV format defined in `src/report/reduction_report.rs` (lines 210-285)
  - ZIP packaging in `src/report/reduction_report.rs` (lines 287-340)
- **User Need**: FR-8 requires "all output files explained with field meanings"

### File Schema Summary
| File Type | Fields | Source Struct | Purpose |
|-----------|--------|---------------|---------|
| Reduced Dataset | (dynamic, original columns minus dropped) | - | Working dataset for modeling |
| reduction_report.json | 8 top-level fields + nested arrays | `ReductionReport` | Machine-readable analysis results |
| gini_analysis.json | 7 fields per feature + bins/categories | `IvAnalysis` | Detailed WoE/IV binning for each feature |
| reduction_report.csv | 10 columns (name, type, iv, gini, correlation_dropped, etc.) | - | Human-readable summary for Excel/BI tools |
| reduction_report.zip | Archive of 3 report files | - | Single artifact for result archival |

### Alternatives Considered
1. **Document Only JSON Format** (skip CSV/ZIP)
   - Rejected: CSV is primary output for non-technical users
   - ZIP is critical for bundling multi-file deliverables
2. **Provide JSON Schema Files** (`.schema.json` files)
   - Rejected: Adds maintenance burden (schemas must stay in sync with structs)
   - Markdown tables are sufficient for this project size
3. **Auto-Generate from Rust Docs** (use serde schema generation)
   - Rejected: Requires custom tooling
   - Doesn't explain business meaning of fields (only technical types)

---

## 6. Test Infrastructure Inventory

### Decision
Document all 163 tests (98 integration + 65 unit) plus 2 benchmark suites with test organization and fixture usage.

### Rationale
- **Test Coverage**: Comprehensive test suite across multiple dimensions:
  - **Integration Tests**: 98 tests in 8 files (`tests/test_*.rs`)
  - **Unit Tests**: 55 tests in 4 source files (`src/pipeline/*.rs` with `#[cfg(test)]` modules)
  - **Benchmarks**: 2 suites in `benches/` (binning performance, correlation performance)
- **Shared Infrastructure**:
  - `tests/common/mod.rs` (184 lines): Shared fixtures and helpers
  - `create_test_dataframe()`: Standard 100-row synthetic dataset
  - Temp file management: `create_temp_csv()`, `create_temp_parquet()`
  - Assertion helpers: `assert_valid_woe()`, `assert_bins_valid()`
- **Test Organization**:
  | Category | File Count | Test Count | Purpose |
  |----------|------------|------------|---------|
  | Integration | 8 | 98 | End-to-end pipeline validation |
  | Unit | 4 | 55 | Function-level correctness |
  | Benchmarks | 2 | 6 scenarios | Performance regression detection |

### Alternatives Considered
1. **Document Only Test Commands** (skip individual test listing)
   - Rejected: Doesn't help developers understand test organization
   - Loses visibility into what's actually tested
2. **Auto-Generate Test List** (parse `cargo test -- --list` output)
   - Rejected: Output is noisy and lacks context about test purpose
   - Better to manually curate with explanations
3. **Coverage Report Integration** (link to `cargo tarpaulin` output)
   - Rejected: Coverage reports show code coverage, not test intent
   - Will mention as supplementary tool, not primary documentation

---

## 7. CLI/TUI Complete Reference

### Decision
Document all 20 main CLI arguments, 1 subcommand (`convert`), and 50+ TUI keyboard shortcuts with three-column layout diagram.

### Rationale
- **CLI Argument Inventory** (from `src/cli/args.rs`, 212 lines):
  - **20 Main Arguments**: `--input`, `--target`, `--missing-threshold`, `--gini-threshold`, `--correlation-threshold`, `--drop`, `--weight`, `--schema-inference-length`, `--binning-strategy`, `--gini-bins`, `--prebins`, `--min-bin-pct`, `--min-category-samples`, `--solver-timeout`, `--solver-gap`, `--trend`, `--no-solver`, `--test-sample-size`, `--output-format`, `--verbose`
  - **1 Subcommand**: `convert` (CSV → Parquet with `--input`, `--output` args)
- **TUI Interface** (from `src/cli/config_menu.rs`, 890 lines):
  - **Three-Column Layout**: THRESHOLDS | SOLVER | DATA
  - **7 Popup Dialogs**: Target selector, threshold editor, solver config, weight selector, drop selector, schema config, file browser
  - **9 Primary Shortcuts**: `[Enter]`, `[T]`, `[F]`, `[D]`, `[C]`, `[S]`, `[W]`, `[A]`, `[Q]`
  - **40+ Navigation Shortcuts**: Arrow keys, Tab, PageUp/Down, Home/End, Esc, dialog-specific keys
- **User Need**: Complete CLI reference (FR-6) must cover both modes (CLI args and TUI interaction)

### Interface Coverage
| Mode | Arguments/Shortcuts | Source | Lines |
|------|---------------------|--------|-------|
| CLI Main | 20 arguments | `args.rs` | 35-185 |
| CLI Subcommand | `convert` with 2 args | `args.rs` | 22-33 |
| TUI Primary | 9 main shortcuts | `config_menu.rs` | 412-561 |
| TUI Navigation | 40+ shortcuts | `config_menu.rs` | 562-890 |

### Alternatives Considered
1. **Document Only Most Common Options** (top 10 arguments)
   - Rejected: Advanced users need complete reference
   - Would omit critical tuning parameters (e.g., `--min-bin-pct`)
2. **Separate CLI and TUI Docs** (two different documents)
   - Rejected: Both are entry points to same pipeline
   - Better to show relationship between CLI args and TUI settings
3. **Auto-Generate from `--help`** (copy help text directly)
   - Rejected: Help text lacks examples and context
   - Manual curation allows better organization and cross-references

---

## 8. Constants and Configuration

### Decision
Create a consolidated constants reference table documenting all 6 critical constants with mathematical significance, defaults, and tuning guidance.

### Rationale
- **Constants Inventory** (from source code analysis):
  | Constant | Value | Location | Purpose |
  |----------|-------|----------|---------|
  | `SMOOTHING` | 0.5 | `src/pipeline/iv.rs:1146` | Laplace smoothing to prevent log(0) in WoE |
  | `MIN_BIN_SAMPLES` | 5 | `src/pipeline/iv.rs:25` | Minimum samples per bin (prevents overfitting) |
  | `DEFAULT_PREBINS` | 20 | `src/pipeline/iv.rs:31` | Initial quantile bins before merging |
  | `DEFAULT_MIN_CATEGORY_SAMPLES` | 5 | `src/pipeline/iv.rs:34` | Minimum samples for categorical bins |
  | `MATRIX_METHOD_COLUMN_THRESHOLD` | 15 | `src/pipeline/correlation.rs:166` | Switch from pairwise to matrix correlation |
  | `TOLERANCE` | 1e-9 | `src/pipeline/correlation.rs:45` | Float comparison precision |
- **Impact on Results**: These constants directly affect:
  - WoE stability (SMOOTHING prevents infinite values)
  - Bin quality (MIN_BIN_SAMPLES prevents single-sample bins)
  - Performance (MATRIX_METHOD_COLUMN_THRESHOLD triggers algorithmic switch)
  - Numerical stability (TOLERANCE for float comparisons)
- **User Need**: Advanced users tuning the pipeline need to understand default behavior

### Alternatives Considered
1. **Document Only in Algorithm Guide** (inline with formulas)
   - Rejected: Users need quick-reference table
   - Algorithm guide focuses on concepts, not configuration
2. **Make All Constants Configurable** (add CLI args for each)
   - Rejected: Would add 6 more CLI arguments
   - These are sensible defaults; changing them is rare
3. **Config File Approach** (TOML/YAML for constants)
   - Rejected: Adds configuration complexity
   - CLI args already provide sufficient tuning for thresholds

---

## 9. Worked Example Strategy

### Decision
Create a small synthetic 20-row dataset with 3 numeric features, 2 categorical features, and 1 binary target, demonstrating all pipeline stages with annotated output at each stage.

### Rationale
- **Spec Requirement**: FR-9 requires "end-to-end worked example with sample data"
- **Coverage Needs**: Example must demonstrate:
  1. **Missing Analysis**: At least one feature with >30% nulls (to show dropping)
  2. **Gini/IV Analysis**: Mix of high-IV and low-IV features (to show Gini threshold)
  3. **Correlation Analysis**: Two highly correlated features (to show correlation dropping)
  4. **Categorical Handling**: Show category grouping and WoE calculation
  5. **Weighted Analysis**: Include weight column to demonstrate weighted WoE/IV
- **Dataset Design**:
  ```
  20 rows × 7 columns:
  - age (numeric, no nulls, high IV)
  - income (numeric, 8 nulls = 40%, will be dropped for missing threshold)
  - debt_ratio (numeric, no nulls, low IV)
  - employment (categorical, 3 categories, high IV)
  - region (categorical, 4 categories, medium IV)
  - default (binary target, 0/1)
  - weight (numeric, sample weights for weighted analysis)

  Correlations: age & income (0.85, high correlation despite income being dropped for nulls)
  ```
- **Output Files to Show**: All 5 output types with annotated excerpts

### Alternatives Considered
1. **Use Real-World Dataset Excerpt** (e.g., first 20 rows of UCI credit data)
   - Rejected: Real data is messy and hard to reason about
   - Synthetic data allows controlled demonstration of each feature
2. **Skip Worked Example** (rely on test data only)
   - Rejected: Violates FR-9
   - Tests don't explain the "why" of each stage
3. **Large Example** (1000+ rows)
   - Rejected: Too verbose for documentation
   - 20 rows is enough to show patterns without overwhelming reader

---

## 10. Cross-Referencing Strategy

### Decision
Use relative file paths with line numbers (e.g., `src/pipeline/iv.rs:166`) for source references and relative links (e.g., `[ADR-002](../adr/002-highs-solver.md)`) for inter-document references.

### Rationale
- **Spec Requirement**: FR-7 requires "cross-references between docs and to source code"
- **Source Code References**:
  - Format: `src/pipeline/iv.rs:166-237` (file path + line range)
  - Enables direct navigation in editors with "Go to File:Line" support (VS Code, IntelliJ, vim)
  - GitHub renders as clickable links in rendered Markdown
- **Inter-Document Links**:
  - Format: `[Link Text](../path/to/doc.md)` or `[Link Text](../path/to/doc.md#section-anchor)`
  - Relative paths work in both GitHub and local file systems
  - Section anchors enable deep linking to specific topics
- **Maintenance**: Line numbers will drift over time, but benefits outweigh maintenance cost:
  - Line ranges provide context even if exact lines change
  - File paths remain stable (modules rarely move)
  - Periodic updates can be done during major refactors

### Cross-Reference Types
| Reference Type | Format | Example | Purpose |
|----------------|--------|---------|---------|
| Source Code | `file:line-range` | `src/pipeline/iv.rs:1146-1158` | Link to implementation |
| Document Section | `[text](path#anchor)` | `[Algorithm Guide](algorithm.md#woe-calculation)` | Link to related concept |
| ADR | `[ADR-NNN](../adr/NNN-title.md)` | `[ADR-002](../adr/002-highs-solver.md)` | Link to decision rationale |
| Constant | `file:line` | `src/pipeline/iv.rs:25` | Link to constant definition |
| Test | `file::test_name` | `tests/test_iv.rs::test_woe_calculation` | Link to test for verification |

### Alternatives Considered
1. **Only Source Code References** (no inter-document links)
   - Rejected: Fragments related information
   - Users would have to manually search for related topics
2. **No Line Numbers** (file paths only)
   - Rejected: Loses precision for large files (e.g., `iv.rs` is 2099 lines)
   - Line numbers provide exact context
3. **Absolute URLs** (full GitHub URLs with commit SHAs)
   - Rejected: Breaks on forks and local file systems
   - Relative paths are portable
4. **Custom Link Checker** (automated tooling to verify links)
   - Considered for future: Could add to CI pipeline
   - Not essential for initial documentation delivery

---

## Summary of Decisions

| Topic | Decision | Key Rationale |
|-------|----------|---------------|
| Structure | `docs/` with `docs/adr/` subdirectory | Spec requirement, GitHub convention |
| Format | Markdown + LaTeX math | GitHub-native rendering (NFR-3) |
| Algorithms | Formulas + edge cases + code refs | 2585 lines of statistical code requires precision |
| ADRs | 8 ADRs with lightweight format | Exceeds spec minimum (5), all have evidence |
| Schemas | All 5 output files documented | Complete coverage of user-facing artifacts |
| Tests | 163 tests + 2 benchmarks | Shows comprehensive test organization |
| CLI/TUI | 21 args + 50+ shortcuts | Complete interface reference (FR-6) |
| Constants | 6 critical constants table | Enables advanced tuning |
| Example | 20-row synthetic dataset | Controlled demonstration of all stages |
| Cross-Refs | File:line + relative links | Direct navigation + portability |

**Next Steps**: Proceed to specification writing with these research findings as foundation.
