<!--
Sync Impact Report
==================
Version change: N/A → 1.0.0 (initial adoption)
Modified principles: N/A (initial)
Added sections: All (Preamble, 5 Principles, Governance)
Removed sections: None
Templates requiring updates:
  - .specify/templates/spec-template.md ✅ created
  - .specify/templates/plan-template.md ✅ created
  - .specify/templates/tasks-template.md ✅ created
Follow-up TODOs: None
-->

# Lo-phi Project Constitution

**Version:** 1.0.0
**Ratification Date:** 2026-01-05
**Last Amended:** 2026-02-01
**Author:** Neels Bester
**Audience:** Data scientists and analysts performing feature reduction

---

## Preamble

Lo-phi is a Rust CLI tool for automated feature reduction in tabular
datasets. It targets data scientists who understand feature engineering
and need a fast, correct, and transparent way to reduce feature sets
using missing-value analysis, Gini/IV (Information Value) scoring,
and correlation filtering.

This constitution defines the non-negotiable principles that govern
all development decisions. Every feature, refactor, and bug fix MUST
be evaluated against these principles.

---

## Principle 1: Statistical Correctness

**All statistical computations MUST produce mathematically correct
results under documented assumptions.**

- WoE/IV binning, Gini coefficients, Pearson correlation, and
  missing-value ratios MUST match their textbook definitions.
- Numerical edge cases (division by zero, empty bins, single-value
  columns, all-null columns) MUST be handled explicitly with
  documented behavior, not silently ignored.
- Smoothing constants (e.g., Laplace smoothing) MUST be documented
  with their rationale and impact on results.
- Any approximation or heuristic (e.g., CART-based binning vs exact
  optimal binning) MUST be clearly labeled in code comments and
  user-facing documentation.
- Test coverage for statistical functions MUST include known-answer
  tests against hand-calculated or reference-implementation values.

**Rationale:** Users make consequential modeling decisions based on
Lo-phi's output. Incorrect statistics silently propagated into
downstream models cause real harm.

---

## Principle 2: Performance at Scale

**Lo-phi MUST remain responsive on datasets with millions of rows
and hundreds of features.**

- Data loading and analysis stages MUST use streaming or chunked
  processing where Polars supports it, avoiding full materialization
  of intermediate DataFrames when possible.
- CPU-bound analysis (IV computation, correlation matrix) MUST use
  parallel processing via Rayon with configurable thread counts.
- Memory allocations in hot paths MUST be minimized; prefer in-place
  mutation and pre-allocated buffers over repeated allocations.
- Performance regressions MUST be detected via Criterion benchmarks.
  Any PR that degrades p95 latency on the benchmark suite by more
  than 10% requires explicit justification.
- The Parquet format SHOULD be preferred over CSV for large datasets,
  and the TUI MUST offer CSV-to-Parquet conversion as a convenience.

**Rationale:** Data scientists work with large datasets. A tool that
chokes on production-sized data is not a tool they will use.

---

## Principle 3: Transparent Decision-Making

**Every feature dropped by Lo-phi MUST be traceable to a specific
analysis rule and threshold.**

- The reduction report MUST include the reason each feature was
  dropped (missing ratio exceeded threshold, IV below threshold,
  or correlation with a retained feature above threshold).
- For correlation drops, the report MUST identify which feature was
  retained and the correlation coefficient.
- The Gini/IV JSON export MUST include per-bin WoE values so users
  can audit binning decisions.
- Default thresholds MUST be documented and overridable via both
  CLI flags and the interactive TUI.
- The pipeline MUST NOT silently skip features or analyses; if a
  stage cannot process a feature, it MUST log a warning with
  the feature name and reason.

**Rationale:** Data scientists need to understand and defend the
feature set they bring to modeling. A black-box reduction tool
undermines trust and reproducibility.

---

## Principle 4: Ergonomic TUI and CLI Experience

**The interactive TUI MUST be intuitive for data scientists who are
not systems programmers.**

- The TUI MUST display all configurable parameters with their current
  values in a scannable layout (currently three-column).
- Keyboard shortcuts MUST be discoverable (displayed on screen) and
  MUST NOT conflict with common terminal bindings.
- Error messages MUST describe what went wrong, which input caused
  it, and what the user can do to fix it.
- CLI arguments MUST have sensible defaults so that a minimal
  invocation (`lophi <file>`) produces useful output.
- File selection MUST support both direct path arguments and
  interactive browsing via the TUI file selector.

**Rationale:** The tool's value is in its analysis, not in forcing
users to memorize arcane flags. A good UX lowers the barrier to
adoption and reduces user errors.

---

## Principle 5: Rigorous Testing Discipline

**Every analysis module MUST have integration tests that exercise
the full pipeline path, and unit tests for edge cases.**

- New features MUST ship with tests covering the happy path and
  at least two edge cases (e.g., empty input, single-row input,
  all-null column, single-category feature).
- Bug fixes MUST include a regression test that fails without the
  fix and passes with it.
- Test fixtures MUST use the shared `tests/common/mod.rs` helpers
  to ensure consistency across test files.
- CI MUST run `cargo clippy --all-targets --all-features -D warnings`,
  `cargo fmt -- --check`, and `cargo test --all-features` on every
  PR. A failing CI gate MUST block merge.
- Benchmarks (`cargo bench`) MUST be maintained for binning and
  correlation modules to catch performance regressions.

**Rationale:** Statistical code is notoriously easy to break with
subtle off-by-one or edge-case errors. Comprehensive testing is
the primary defense against shipping incorrect results.

---

## Governance

### Amendment Procedure

1. Propose a change as a PR modifying this file.
2. The PR description MUST state which principle is affected, why
   the change is needed, and the version bump rationale.
3. All amendments require review and approval by the project author.
4. After merge, update `Last Amended` date and `Version` per the
   versioning policy below.

### Versioning Policy

This constitution follows semantic versioning:

- **MAJOR:** Backward-incompatible governance changes (principle
  removals, fundamental redefinitions).
- **MINOR:** New principles added, material expansions to existing
  principles.
- **PATCH:** Clarifications, wording improvements, typo fixes.

### Compliance Review

- Every PR SHOULD be evaluated against applicable principles before
  merge.
- The CLAUDE.md file MUST be kept in sync with architectural changes
  per Principle 3 (transparency) and Principle 5 (testing).
- Quarterly (or per-release) review of this constitution is
  RECOMMENDED to ensure principles reflect project evolution.

### Pre-1.0 Output Format Policy

Lo-phi is currently at version 0.x. Output formats (JSON reports,
CSV summaries, reduced dataset schemas) MAY change between versions
without backward-compatibility guarantees. Users SHOULD pin to a
specific Lo-phi version if they depend on output format stability.
