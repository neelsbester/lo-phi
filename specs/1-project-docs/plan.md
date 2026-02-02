# Implementation Plan: Comprehensive Project Reference Documentation

**Spec Version:** 1.0.0
**Date:** 2026-02-01

---

## Constitution Check

Before implementation, verify alignment with:

- [x] **Principle 1 (Statistical Correctness):** All documented formulas (WoE, IV, Gini, Pearson, Welford) must exactly match implementations in `src/pipeline/iv.rs`, `src/pipeline/correlation.rs`. Constants (`SMOOTHING=0.5`, `MIN_BIN_SAMPLES=5`, etc.) verified against source. Edge-case handling (zero events, all-null columns, single-value features) explicitly documented.
- [N/A] **Principle 2 (Performance):** Documentation is a one-time authoring task with no runtime impact. No performance-sensitive code changes.
- [x] **Principle 3 (Transparency):** All architectural decisions documented via 8 ADRs. Pipeline flow documented end-to-end. Output schemas provide field-level traceability. Cross-references link decisions to source code.
- [x] **Principle 4 (Ergonomic UX):** Complete CLI/TUI reference covers all 21 CLI arguments and 50+ TUI shortcuts. User Guide organized for data scientists. Glossary makes domain terms accessible.
- [x] **Principle 5 (Testing):** Test infrastructure documented (163 tests, 2 benchmark suites). Developer Guide covers test architecture, fixtures, and how to write new tests. No code changes = no new tests needed.

## Architecture Overview

This feature creates a pure-documentation deliverable: 15 Markdown files organized in a `docs/` directory at the repository root. No code changes are required. All documents use standard GitHub-compatible Markdown with LaTeX math blocks for formulas.

### Document Dependency Graph

```
glossary.md (no dependencies)
    |
architecture.md (depends: glossary)
    |
    +-- algorithms.md (depends: glossary, architecture)
    |       |
    |       +-- output-reference.md (depends: algorithms, glossary)
    |
    +-- user-guide.md (depends: glossary)
    |
    +-- developer-guide.md (depends: architecture, user-guide, algorithms)
    |
    +-- worked-example.md (depends: all above)
    |
    +-- adr/*.md (minimal dependencies, can be parallel)
```

### Target File Structure

```
docs/
├── architecture.md          # FR-1: System architecture
├── algorithms.md            # FR-2: Statistical methods
├── user-guide.md            # FR-3: CLI/TUI reference
├── developer-guide.md       # FR-4: Developer onboarding
├── output-reference.md      # FR-5: Output file formats
├── glossary.md              # FR-8: Domain terms
├── worked-example.md        # FR-9: End-to-end example
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

## Implementation Steps

### Phase 0: Foundation Documents

1. **Create `docs/` directory structure**
   - Files: `docs/`, `docs/adr/`
   - Dependencies: None

2. **Write `docs/glossary.md`** (FR-8)
   - Define ~20 domain terms: WoE, IV, Gini, Binning, CART, Quantile, Laplace Smoothing, Monotonicity, Null Ratio, Pearson Correlation, Welford Algorithm, Event Rate, Solver, Gap Tolerance, Schema Inference, Feature Reduction, Pre-binning, Population Splitting
   - Format: Alphabetical, each with definition + context + related terms
   - Source references: Terms extracted from `src/pipeline/iv.rs`, `src/pipeline/correlation.rs`, `src/pipeline/missing.rs`
   - Estimated: 800-1200 words
   - Dependencies: None

3. **Write `docs/architecture.md`** (FR-1)
   - System overview with text-based module diagram
   - Module responsibilities: `src/cli/`, `src/pipeline/`, `src/report/`, `src/utils/`
   - Pipeline flow: Config → Load → Missing → Gini/IV → Correlation → Save → Report
   - Key design patterns: Builder (ReductionReportBuilder), Strategy (BinningStrategy), Parallel processing (Rayon)
   - Technology stack with Cargo.toml dependency inventory
   - Source references: `src/main.rs`, all `mod.rs` files, `Cargo.toml`
   - Links to ADRs for decision rationale
   - Estimated: 1200-1500 words
   - Dependencies: glossary.md

### Phase 1: Core Reference Guides (parallelizable)

4. **Write `docs/algorithms.md`** (FR-2) -- CRITICAL PATH
   - WoE formula: `WoE = ln((events+0.5)/(total_events+0.5) / (non_events+0.5)/(total_non_events+0.5))`
   - IV formula: `IV = Σ(dist_events - dist_non_events) × WoE`
   - Gini impurity: `2 × p × (1-p)` for CART splits
   - Gini coefficient: `2 × AUC - 1` via weighted Mann-Whitney U
   - Pearson correlation via Welford single-pass algorithm
   - CART binning: recursive split finding, information gain, min sample constraints
   - Quantile binning: equal-frequency approach
   - Bin merging: greedy (min IV loss) and solver-based (HiGHS MIP)
   - Monotonicity constraints: None, Ascending, Descending, Peak, Valley, Auto
   - Missing value handling: separate MISSING bin with WoE
   - Categorical features: rare category merging into "OTHER"
   - Constants table: SMOOTHING=0.5, MIN_BIN_SAMPLES=5, DEFAULT_PREBINS=20, DEFAULT_MIN_CATEGORY_SAMPLES=5, MATRIX_METHOD_COLUMN_THRESHOLD=15, TOLERANCE=1e-9
   - Source references: `src/pipeline/iv.rs` (~2600 lines), `src/pipeline/correlation.rs` (~485 lines), `src/pipeline/missing.rs`, `src/pipeline/target.rs`, `src/pipeline/solver/`
   - Estimated: 2000-2500 words (most complex document)
   - Dependencies: glossary.md, architecture.md

5. **Write `docs/user-guide.md`** (FR-3)
   - All 21 CLI arguments with name, type, default, description, valid range (from `src/cli/args.rs`)
   - `convert` subcommand with 4 arguments
   - TUI three-column layout diagram: THRESHOLDS | SOLVER | DATA
   - All keyboard shortcuts: 9 primary (`[Enter]`, `[T]`, `[F]`, `[D]`, `[C]`, `[S]`, `[W]`, `[A]`, `[Q]`) + navigation keys
   - 7 popup dialogs: target selector, drop selector, threshold editor, solver toggle, monotonicity selector, weight selector, schema editor
   - TUI-configurable vs CLI-only parameters table
   - Common workflows with examples
   - Source references: `src/cli/args.rs`, `src/cli/config_menu.rs`, `src/cli/convert.rs`
   - Estimated: 1500-1800 words
   - Dependencies: glossary.md

### Phase 2: Developer & Output Documentation (parallelizable)

6. **Write `docs/developer-guide.md`** (FR-4)
   - Prerequisites: Rust toolchain (stable), git
   - Build commands: `cargo build`, `cargo build --release`
   - Test commands: `cargo test --all-features`, `make check`, unit vs integration
   - Test structure: 163 tests (98 integration in `tests/test_*.rs`, 65 unit in source)
   - Shared fixtures: `tests/common/mod.rs` with `create_test_dataframe()`, temp file helpers, assertion helpers
   - Benchmarks: `cargo bench` with Criterion (`benches/binning_benchmark.rs`, `benches/correlation_benchmark.rs`)
   - CI: GitHub Actions on PRs to main (Ubuntu test + macOS/Windows build)
   - Code conventions: `cargo fmt`, `cargo clippy -D warnings`, `anyhow`/`thiserror` error handling
   - Contributing guide: commit message format (`fix:`, `feature:`, `docs:`, `chore:`), PR process
   - How to add new pipeline stage, new CLI option, new TUI shortcut
   - Source references: `Cargo.toml`, `Makefile`, `tests/`, `benches/`, `.github/workflows/`
   - Estimated: 1800-2200 words
   - Dependencies: architecture.md, user-guide.md, algorithms.md

7. **Write `docs/output-reference.md`** (FR-5)
   - Reduced dataset (`{input}_reduced.{csv|parquet}`): retained columns, row preservation
   - Reduction report JSON: `ReductionReport` struct with `metadata`, `summary`, `features[]` schemas
     - `ReportMetadata`: timestamp, version, thresholds, settings
     - `ReportSummary`: initial/final features, per-stage drops, timing
     - `FeatureReportEntry`: name, status, stage, reason, analysis (missing/gini/correlation)
   - Gini analysis JSON: `IvAnalysis` with `WoeBin`, `CategoricalWoeBin`, `MissingBin` field-level schemas
   - Reduction report CSV: 10 columns (feature, status, stage, reason, missing_ratio, gini, iv, feature_type, max_correlation, correlated_with)
   - ZIP bundle: Deflate compression, 0o644 permissions, cleanup after packaging
   - Example snippets for each format
   - Source references: `src/report/reduction_report.rs`, `src/report/gini_export.rs`, `src/report/summary.rs`, `src/pipeline/iv.rs` (struct definitions)
   - Estimated: 1200-1500 words
   - Dependencies: algorithms.md, glossary.md

### Phase 3: Synthesis

8. **Write `docs/worked-example.md`** (FR-9)
   - Create synthetic 20-row dataset with:
     - 3 numeric features (1 high-IV, 1 low-IV, 1 high-missing)
     - 2 categorical features (1 high-IV, 1 medium-IV)
     - 1 binary target column
   - Document configuration: thresholds (0.30/0.05/0.40), CART binning, solver enabled
   - Walk through each pipeline stage with annotated results:
     - Missing analysis: show which feature exceeds threshold
     - Gini/IV analysis: show WoE bins, IV values, which features dropped
     - Correlation analysis: show correlated pair, which feature retained
   - Show annotated excerpts from all 5 output files
   - Cross-reference formulas to algorithms.md, fields to output-reference.md
   - Source: Run Lo-phi on synthetic dataset, capture actual output
   - Estimated: 1500-2000 words
   - Dependencies: All previous documents

### Phase 4: Architectural Decision Records (parallelizable anytime)

9. **Write 8 ADRs** (FR-6)
   - Use `contracts/adr-template.md` format: Title / Status / Date / Context / Decision / Alternatives / Consequences / References
   - Priority order:
     1. ADR-001: Polars Framework (referenced by architecture.md)
     2. ADR-006: Sequential Pipeline (referenced by architecture.md)
     3. ADR-008: Ratatui TUI (referenced by architecture.md)
     4. ADR-002: HiGHS Solver
     5. ADR-003: CART Default Binning
     6. ADR-004: WoE Convention
     7. ADR-005: Welford Correlation
     8. ADR-007: Dual File Format
   - Each ADR: 200-400 words with ≥2 alternatives considered, both positive and negative consequences
   - Source references: `Cargo.toml`, relevant implementation files, git commit history
   - Estimated: 1600-3200 words total
   - Dependencies: Minimal (can run in parallel with other phases)

### Phase 5: Cross-Reference Verification & CLAUDE.md Update

10. **Verify all cross-references**
    - Check all `[link](path)` references resolve
    - Check all `src/file.rs:line` references exist
    - Verify glossary terms are defined for all terms used in docs
    - Ensure FR-7 (cross-referencing) is satisfied
    - Dependencies: All documents complete

11. **Update CLAUDE.md**
    - Add documentation suite reference under project overview
    - Reference `docs/` directory
    - Do not duplicate content (point to docs for details)
    - Dependencies: All documents complete

## Testing Strategy

- **Unit tests:** N/A (no code changes)
- **Integration tests:** N/A (no code changes)
- **Validation tests (manual):**
  - All formulas in algorithms.md match `src/pipeline/iv.rs` and `src/pipeline/correlation.rs`
  - All constants match source values
  - All CLI arguments from `src/cli/args.rs` are documented in user-guide.md
  - All TUI shortcuts from `src/cli/config_menu.rs` are documented
  - JSON schemas match Rust struct serialization (`serde` derive)
  - Worked example is reproducible
  - All Markdown renders correctly on GitHub (no broken LaTeX, no broken links)
- **Scenario validation:**
  - Scenario 1: New contributor can set up and submit PR using only docs
  - Scenario 2: Data scientist can interpret all output fields using only docs + glossary
  - Scenario 3: Auditor can assess methodology using only docs + ADRs
  - Scenario 4: Developer can implement new pipeline stage following documented patterns

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Formula documentation errors | High | Extract from source code, verify with synthetic data runs |
| Source code changes during documentation | Medium | Work on `1-project-docs` branch, verify against latest commit before merge |
| Cross-reference link rot | Low | Use relative paths, validate before publishing |
| Worked example not reproducible | Medium | Include synthetic dataset CSV, document exact CLI command |
| Documentation staleness post-merge | Medium | Add maintenance triggers to CLAUDE.md (update docs when code changes) |
| ADR strawman alternatives | Low | Require ≥2 realistic alternatives per ADR with specific rejection reasons |

## Rollback Plan

Documentation is additive only (new `docs/` directory). Rollback is simply:

```bash
git rm -r docs/
git commit -m "revert: remove documentation suite"
```

No code changes are involved, so no risk of breaking existing functionality.

---

## Summary

| Metric | Value |
|--------|-------|
| Total documents | 16 (7 guides + 8 ADRs + 1 STATUS tracker) |
| Total estimated words | 11,300-15,900 |
| Critical path | glossary → architecture → algorithms → output-reference → worked-example |
| Parallel phases | Phase 1 (algorithms + user-guide), Phase 2 (developer + output), Phase 4 (all ADRs) |
| Code changes | None |
| New tests | None |
| FR coverage | FR-1 through FR-9 fully mapped |
| NFR coverage | NFR-1 (accuracy via source verification), NFR-2 (modular files), NFR-3 (Markdown on GitHub), NFR-4 (completeness via validation) |
