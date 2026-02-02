# Tasks: Comprehensive Project Reference Documentation

**Plan Version:** 1.0.0
**Date:** 2026-02-01

---

## Task Categories

Tasks are categorized by the constitution principle they primarily serve:

- **STAT:** Statistical Correctness (Principle 1)
- **TRANS:** Transparent Decision-Making (Principle 3)
- **UX:** Ergonomic TUI/CLI (Principle 4)
- **TEST:** Rigorous Testing (Principle 5)

> Note: Principle 2 (Performance) is N/A for this documentation-only feature.

## User Story Mapping

User stories are derived from the spec's user scenarios:

| Story | Scenario | Primary Documents |
|-------|----------|-------------------|
| US1 | New Contributor Onboarding | developer-guide, architecture, algorithms, glossary |
| US2 | Data Scientist Understanding Results | output-reference, algorithms, glossary, worked-example |
| US3 | Auditor Reviewing Decision Logic | algorithms, ADRs, user-guide, architecture |
| US4 | Developer Adding New Analysis Stage | architecture, developer-guide, algorithms, ADRs |

## Tasks

### Phase 1: Setup

- [x] T001 Create `docs/` and `docs/adr/` directory structure at repository root
- [x] T002 Create initial `docs/STATUS.md` tracking table with all 15 deliverable documents (7 guides + 8 ADRs) listed as Draft status per `contracts/doc-structure.md`

---

### Phase 2: Foundation (blocks all user stories)

- [x] T003 Write `docs/glossary.md` (FR-8) defining ~23 domain terms (WoE, IV, Gini, CART, Quantile, Laplace Smoothing, Monotonicity, Null Ratio, Pearson Correlation, Welford Algorithm, Event Rate, Solver, Gap Tolerance, Schema Inference, Feature Reduction, Prebinning, Population Splitting, Binning, Bad Rate, Good Rate, MIP, Weighted Analysis, Cramér's V) in alphabetical order. Each entry must include: definition, context in Lo-phi, related terms, formula (if applicable). Source terms from `src/pipeline/iv.rs`, `src/pipeline/correlation.rs`, `src/pipeline/missing.rs`. Target: 800-1200 words.

- [x] T004 Write `docs/architecture.md` (FR-1) covering: (1) System overview with text-based module diagram, (2) Module structure for `src/cli/`, `src/pipeline/`, `src/report/`, `src/utils/`, (3) Pipeline flow: Config -> Load -> Missing -> Gini/IV -> Correlation -> Save -> Report, (4) Design patterns: Builder (ReductionReportBuilder), Strategy (BinningStrategy), Parallel processing (Rayon), (5) Technology stack from `Cargo.toml`. Must link to ADRs (001, 006, 008), `algorithms.md`, `developer-guide.md`, and `glossary.md`. Source: `src/main.rs`, all `mod.rs` files, `Cargo.toml`. Target: 1200-1500 words.

---

### Phase 3: Core Technical Documents (US1, US2, US3, US4)

- [x] T005 [P] [US2] Write `docs/algorithms.md` (FR-2) covering: (1) WoE formula with Laplace smoothing (`SMOOTHING=0.5`), (2) IV formula and interpretation thresholds, (3) Gini impurity (`2*p*(1-p)`) and Gini coefficient (`2*AUC-1`), (4) CART binning with recursive split finding and min sample constraints, (5) Quantile binning, (6) Bin merging: greedy and solver-based (HiGHS MIP), (7) Monotonicity constraints (None, Ascending, Descending, Peak, Valley, Auto), (8) Missing value handling (separate MISSING bin), (9) Categorical features (rare category merging), (10) Pearson correlation via Welford single-pass algorithm, (11) Constants table: `SMOOTHING=0.5`, `MIN_BIN_SAMPLES=5`, `DEFAULT_PREBINS=20`, `DEFAULT_MIN_CATEGORY_SAMPLES=5`, `MATRIX_METHOD_COLUMN_THRESHOLD=15`, `TOLERANCE=1e-9`. All formulas in LaTeX. Source: `src/pipeline/iv.rs`, `src/pipeline/correlation.rs`, `src/pipeline/missing.rs`, `src/pipeline/target.rs`, `src/pipeline/solver/`. Target: 2000-2500 words. CRITICAL: verify every formula against source code. Include cross-references to `glossary.md` for all domain terms and to `architecture.md` for module context (FR-7). Document warning/error behavior when a feature cannot be processed (per Constitution Principle 3: no silent skipping).

- [x] T006 [P] [US3] Write `docs/user-guide.md` (FR-3) covering: (1) Installation and quick start, (2) All 21 CLI arguments (including `event_value`, `non_event_value`, `output`, `no_confirm`) with name, type, default, description, valid range from `src/cli/args.rs`, (3) `convert` subcommand with arguments from `src/cli/convert.rs`, (4) TUI three-column layout diagram (THRESHOLDS | SOLVER | DATA), (5) All keyboard shortcuts: 9 primary (`[Enter]`, `[T]`, `[F]`, `[D]`, `[C]`, `[S]`, `[W]`, `[A]`, `[Q]`) plus navigation keys from `src/cli/config_menu.rs`, (6) 7 popup dialogs, (7) TUI-configurable vs CLI-only parameters table, (8) Common workflows with example commands. Source: `src/cli/args.rs`, `src/cli/config_menu.rs`, `src/cli/convert.rs`. Target: 1500-1800 words.

---

### Phase 4: Reference & Developer Documentation (US1, US2, US4)

- [x] T007 [P] [US2] Write `docs/output-reference.md` (FR-5) covering: (1) Reduced dataset (`{input}_reduced.{csv|parquet}`): retained columns, row preservation, (2) Reduction report JSON: `ReductionReport` struct with `metadata`, `summary`, `features[]` field-level schemas from `src/report/reduction_report.rs`, (3) Gini analysis JSON: `IvAnalysis` with `WoeBin`, `CategoricalWoeBin`, `MissingBin` field-level schemas from `src/pipeline/iv.rs`, (4) Reduction report CSV: 10 columns with pipe-separated correlation format, (5) ZIP bundle: Deflate compression packaging, (6) Example snippets for each format, (7) Interpreting results section (IV thresholds, WoE meaning, correlation). Source: `src/report/reduction_report.rs`, `src/report/gini_export.rs`, `src/report/summary.rs`, `src/pipeline/iv.rs` (struct defs). Target: 1200-1500 words.

- [x] T008 [P] [US1] Write `docs/developer-guide.md` (FR-4) covering: (1) Prerequisites: Rust toolchain, git, (2) Build: `cargo build`, `cargo build --release`, (3) Testing: `cargo test --all-features`, `make check`, unit vs integration, (4) Test structure: 163 tests (98 integration in `tests/test_*.rs`, 65 unit), (5) Shared fixtures: `tests/common/mod.rs` with `create_test_dataframe()`, temp file helpers, assertion helpers, (6) Benchmarks: `cargo bench` with Criterion suites, (7) CI: GitHub Actions (Ubuntu test + macOS/Windows build), (8) Code conventions: `cargo fmt`, `cargo clippy -D warnings`, `anyhow`/`thiserror`, (9) Contributing: commit prefixes (`fix:`, `feature:`, `docs:`, `chore:`), PR process, (10) How to add: new pipeline stage, new CLI option, new TUI shortcut. Source: `Cargo.toml`, `Makefile`, `tests/`, `benches/`, `.github/workflows/`. Target: 1800-2200 words.

---

### Phase 5: Architectural Decision Records (US3, US4)

- [x] T009 [P] [US3] Write `docs/adr/ADR-001-polars-framework.md` using `contracts/adr-template.md`. Context: need for high-performance DataFrame operations with CSV/Parquet support. Decision: Polars. Alternatives: pandas (Python, not Rust-native), DataFusion (query-engine focus, heavier), ndarray (too low-level). Source: `Cargo.toml` (polars v0.45). Target: 250-350 words.

- [x] T010 [P] [US3] Write `docs/adr/ADR-002-highs-solver.md` using `contracts/adr-template.md`. Context: need MIP solver for monotonic binning optimization. Decision: HiGHS via `good_lp` crate. Alternatives: CBC (slower, C++ dep), GLPK (GPL license), custom solver (too complex). Source: `Cargo.toml`, `src/pipeline/solver/`. Target: 250-350 words.

- [x] T011 [P] [US3] Write `docs/adr/ADR-003-cart-default-binning.md` using `contracts/adr-template.md`. Context: choosing default binning strategy. Decision: CART default (quantile available via CLI). Alternatives: Quantile (simpler but worse separation), fixed-width (too simplistic). Source: `src/pipeline/iv.rs:166-237`. Target: 200-300 words.

- [x] T012 [P] [US3] Write `docs/adr/ADR-004-woe-convention.md` using `contracts/adr-template.md`. Context: multiple WoE sign conventions in literature. Decision: `ln(Bad/Good)` convention (i.e., `ln(%bad/%good)`) where positive WoE indicates higher risk. Alternatives: `ln(Good/Bad)` (positive WoE = lower risk, less common in credit scoring). Source: `src/pipeline/iv.rs:1448-1467`. Target: 200-300 words.

- [x] T013 [P] [US3] Write `docs/adr/ADR-005-welford-correlation.md` using `contracts/adr-template.md`. Context: numerical stability for Pearson correlation. Decision: Welford single-pass algorithm. Alternatives: two-pass algorithm (2x memory reads), naive formula (numerically unstable). Source: `src/pipeline/correlation.rs:166-286`. Target: 200-300 words.

- [x] T014 [P] [US3] Write `docs/adr/ADR-006-sequential-pipeline.md` using `contracts/adr-template.md`. Context: pipeline stage ordering. Decision: Missing -> IV/Gini -> Correlation sequential. Alternatives: parallel stages (order-dependent issues), configurable ordering (complexity). Source: `src/main.rs:77-200`. Target: 250-350 words.

- [x] T015 [P] [US3] Write `docs/adr/ADR-007-dual-file-format.md` using `contracts/adr-template.md`. Context: input/output format support. Decision: Both CSV and Parquet. Alternatives: CSV only (no performance), Parquet only (less accessible). Source: `src/pipeline/loader.rs`, `src/cli/convert.rs`. Target: 200-300 words.

- [x] T016 [P] [US3] Write `docs/adr/ADR-008-ratatui-tui.md` using `contracts/adr-template.md`. Context: interactive configuration without GUI. Decision: Ratatui terminal UI. Alternatives: CLI-only (less user-friendly), web UI (too complex), egui (requires GUI). Source: `src/cli/config_menu.rs`. Target: 200-300 words.

---

### Phase 6: Synthesis (US2)

- [x] T017a [US2] Create synthetic dataset `docs/examples/synthetic_data.csv` (20 rows, 3 numeric features, 2 categorical features, 1 binary target). Run Lo-phi on the dataset with configuration (thresholds 0.30/0.05/0.40, CART binning, solver enabled) and capture all output files to `docs/examples/`. These outputs are required by T017 which must use actual Lo-phi output.

- [x] T017 [US2] Write `docs/worked-example.md` (FR-9) covering: (1) Create synthetic 20-row dataset CSV with 3 numeric features (1 high-IV, 1 low-IV, 1 high-missing), 2 categorical features (1 high-IV, 1 medium-IV), 1 binary target, (2) Document configuration: thresholds (0.30/0.05/0.40), CART binning, solver enabled, (3) Walk through each pipeline stage with annotated results: missing analysis (show feature exceeding threshold), Gini/IV analysis (show WoE bins, IV values, dropped features), correlation analysis (show correlated pair, retained feature), (4) Show annotated excerpts from all 5 output file types, (5) Cross-reference formulas to `algorithms.md`, fields to `output-reference.md`, parameters to `user-guide.md`. Must use actual Lo-phi output (not fabricated). Target: 1500-2000 words.

---

### Phase 7: Polish & Cross-Cutting Concerns

- [x] T018 Verify all cross-references across `docs/` satisfy FR-7: check all `[link](path)` references resolve, check all `src/file.rs:line` references exist, verify glossary terms are defined for all terms used in docs, verify all inter-document links work. Verify LaTeX math blocks render correctly on GitHub by pushing a test branch and viewing rendered Markdown.
- [x] T019 Update `CLAUDE.md` to reference `docs/` documentation suite under project overview section. Add brief description pointing to `docs/` for detailed documentation. Do not duplicate content.
- [x] T020 Update `docs/STATUS.md` to reflect final document states (Published) with verified commit hash

---

## Dependency Graph

```
T001 (setup) ─→ T002 (STATUS.md)
                  ↓
T003 (glossary) ─→ T004 (architecture)
                      ↓
         ┌────────────┼────────────┐
         ↓            ↓            ↓
    T005 (algorithms) T006 (user-guide) T009-T016 (ADRs) [P]
         ↓            ↓
    ┌────┼────┐       ↓
    ↓         ↓       ↓
T007 (output) T008 (developer)
    ↓         ↓
    └────┬────┘
         ↓
    T017 (worked-example)
         ↓
    T018 (cross-ref verify)
         ↓
    ┌────┼────┐
    ↓         ↓
T019 (CLAUDE) T020 (STATUS)
```

## Parallel Execution Opportunities

### Phase 3 (after Phase 2 completes):
- **T005** (algorithms.md) and **T006** (user-guide.md) can run in parallel
- Both depend only on T003 (glossary) and T004 (architecture)

### Phase 4 (after Phase 3 completes):
- **T007** (output-reference.md) and **T008** (developer-guide.md) can run in parallel
- T007 depends on T005 (algorithms); T008 depends on T004 + T005 + T006

### Phase 5 (after T004 completes):
- All 8 ADRs (**T009-T016**) can run in parallel with each other
- ADRs can also run in parallel with Phase 3 and Phase 4 tasks
- Only soft dependency on T004 (architecture) for context

### Maximum parallelism:
- After T004 completes: T005 + T006 + T009-T016 (up to 10 tasks simultaneously)
- After T005 + T006 complete: T007 + T008 in parallel

## Implementation Strategy

### MVP Scope (User Story 2: Data Scientist)
Complete these tasks for minimum viable documentation:
1. T001-T002 (setup)
2. T003 (glossary)
3. T004 (architecture)
4. T005 (algorithms) -- critical path
5. T007 (output-reference)
6. T017 (worked-example)

This covers the data scientist scenario end-to-end: understanding algorithms, interpreting outputs, and following a worked example.

### Incremental Delivery
1. **Increment 1 (MVP):** Foundation + algorithms + output-reference + worked-example
2. **Increment 2:** user-guide + developer-guide (enables US1 and US3)
3. **Increment 3:** 8 ADRs (enables US3 and US4)
4. **Increment 4:** Cross-reference verification + CLAUDE.md update + STATUS.md finalization

### Critical Path
```
T001 → T003 → T004 → T005 → T007 → T017 → T018 → T019/T020
```

Duration determined by: glossary + architecture + algorithms + output-reference + worked-example + verification (sequential dependency chain).

---

## Completion Checklist

- [ ] All tasks marked Done
- [ ] All 15 documents exist in `docs/`
- [ ] Word count within estimated ranges (11,300-15,900 total)
- [ ] All cross-references resolve (FR-7)
- [ ] All formulas verified against source code (NFR-1)
- [ ] All CLI arguments from `src/cli/args.rs` documented (NFR-4)
- [ ] All TUI shortcuts from `src/cli/config_menu.rs` documented (NFR-4)
- [ ] All output schemas match Rust struct serialization (NFR-1)
- [ ] Worked example is reproducible (FR-9)
- [ ] Markdown renders correctly on GitHub (NFR-3)
- [ ] CLAUDE.md updated to reference docs/ (maintenance)
- [ ] Constitution compliance verified (Principles 1, 3, 4, 5)
