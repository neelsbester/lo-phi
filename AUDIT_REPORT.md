# Lo-phi Codebase Audit Report

**Date:** 2026-03-01
**Scope:** Full codebase (~17,874 lines across 33 source files)
**Audit Dimensions:** Architecture, Code Quality, Security, Performance, Error Handling, Test Coverage

---

## Executive Summary

Lo-phi is a well-engineered Rust CLI tool with **zero unsafe code**, strong memory safety guarantees, and generally solid architecture. The codebase demonstrates good Rust idioms, effective parallelism via Rayon, and a clean pipeline design. However, the audit identified **47 findings** across 6 dimensions that warrant attention before the next release.

### Findings by Severity

| Severity | Count | Key Themes |
|----------|-------|------------|
| Critical | 4 | Terminal state restoration, integer underflow, Config duplication, `process::exit` bypasses cleanup |
| High | 12 | OOM via crafted SAS headers, silent error swallowing, DataFrame clone waste, monolithic iv.rs |
| Medium | 16 | Missing CLI validation, float comparison, non-deterministic HashMap iteration, missing tests |
| Low | 15 | Dead code annotations, cosmetic issues, minor optimizations |

### Top 5 Priority Fixes

1. **Terminal not restored on error paths** (wizard.rs, config_menu.rs) -- leaves user terminal broken
2. **`std::process::exit(0)` in 5 places** -- bypasses RAII cleanup, untestable
3. **OOM via crafted SAS7BDAT headers** -- no sanity bounds on allocation sizes
4. **Silently swallowed IV/correlation errors** -- features disappear without warning
5. **Missing CLI threshold validation** -- allows nonsensical values like `--missing-threshold -1.0`

---

## 1. Architecture

### Critical

| ID | Finding | Location |
|----|---------|----------|
| A-C1 | **Duplicated `Config` / `PipelineConfig` structs** with near-identical fields. Adding a parameter requires changes in 3+ places. Should unify or use `TryFrom<Config>`. | `config_menu.rs:28-66`, `main.rs:38-66`, `main.rs:252-278` |
| A-C2 | **`std::process::exit(0)` called in 5 places** instead of returning errors. Bypasses RAII cleanup, makes code untestable. | `main.rs:223,426,446,450,575` |

### Significant

| ID | Finding | Location |
|----|---------|----------|
| A-S1 | **`pub use *` glob re-exports** leak internal APIs (`find_correlated_pairs`, `create_target_mask`, `total_weight`). Should use explicit re-exports. | `pipeline/mod.rs:12-18`, `report/mod.rs:7-9` |
| A-S2 | **Path derivation pattern repeated 7 times.** Should extract a `derive_output_path()` helper. | `main.rs:153-190,229-246,384-397,683-690` |
| A-S3 | **`iv.rs` at 2613 lines is monolithic.** Handles types, CART, quantile, WoE, categorical, merging, solver integration, and tests. Should be split into 5-6 sub-modules. | `pipeline/iv.rs` |
| A-S4 | **`config_menu.rs` (2652 lines) and `wizard.rs` (2431 lines)** have overlapping rendering logic (logo, box layout, list selection, search). No shared UI helpers. | `cli/config_menu.rs`, `cli/wizard.rs` |
| A-S5 | **DataFrame cloned unnecessarily for `drop_many`.** Full dataset copy that is immediately discarded. | `main.rs:620,765` |

### Minor

| ID | Finding | Location |
|----|---------|----------|
| A-M1 | `lib.rs` makes all modules public but no external consumers exist. Consider `pub(crate)`. | `src/lib.rs` |
| A-M2 | Unused dependency `dialoguer` in Cargo.toml -- superseded by Ratatui TUI. | `Cargo.toml:33` |
| A-M3 | Inconsistent `SasError`-to-`anyhow` conversion. Uses string formatting instead of `From` impl. | `loader.rs:37,160` |
| A-M4 | `solver/model.rs` reaches two levels up with `super::super::iv::WoeBin`. `WoeBin` should be in shared types. | `solver/model.rs:14` |

### Positive

- Clean pipeline orchestration: Config -> Load -> Missing -> Gini -> Correlation -> Save -> Report
- Excellent SAS7BDAT parser internal architecture (8 well-separated sub-modules)
- Strict downward dependency flow with no circular dependencies
- Well-designed solver module with clean public API and hidden internals
- Smart auto-selection of correlation method (pairwise vs matrix at threshold 15)

---

## 2. Code Quality

### High

| ID | Finding | Location |
|----|---------|----------|
| Q-H1 | **DataFrame clone for `drop_many`** -- unnecessary full copy doubling peak memory. | `main.rs:620,765` |
| Q-H2 | **Guarded unwrap anti-pattern** (`is_none() \|\| .unwrap()`). Should use `map_or` or `match`. | `reduction_report.rs:288`, `solver/model.rs:116` |
| Q-H3 | **50+ `#[allow(dead_code)]` suppressions** across wizard.rs, solver, sas7bdat. Many may be genuinely unused code. | `wizard.rs` (12), `sas7bdat/constants.rs` (16), `solver/mod.rs` (4) |
| Q-H4 | **`#[allow(clippy::too_many_arguments)]`** on 6 functions. `analyze_features_iv` takes 11 parameters. Should use config structs. | `iv.rs:424,659,884,1112`, `solver/model.rs:81,126` |

### Medium

| ID | Finding | Location |
|----|---------|----------|
| Q-M1 | **Stale `#[allow(unused_imports)]`** with comment referencing "Phase 3/4 when wizard is integrated" -- wizard IS integrated. | `cli/mod.rs:13` |
| Q-M2 | **`weights.to_vec()` for Arc wrapping** -- full copy of weights slice. Could pass `Arc<[f64]>` from caller. | `iv.rs:749`, `correlation.rs:81` |
| Q-M3 | **String-typed enums** for `binning_strategy` and `monotonicity` in `PipelineConfig`. Parsed from strings at multiple points. Should store as enum types after initial parsing. | `main.rs:53,109-112,644-654` |
| Q-M4 | **HashMap iteration order non-determinism** in categorical IV analysis. Which categories merge into OTHER could vary between runs. | `iv.rs:1225` |
| Q-M5 | **Floating-point comparison with `==`** on std_x/std_y and total_weight. Should use epsilon tolerance. | `correlation.rs:175`, `missing.rs:27` |
| Q-M6 | **`clone()` on Config** in dashboard loop every iteration. | `main.rs:354` |

### Low

| ID | Finding | Location |
|----|---------|----------|
| Q-L1 | Single `TODO` comment in production: "Full Windows-1252 mapping for 0x80-0x9F range". | `sas7bdat/column.rs:148` |
| Q-L2 | Unused parameters with underscore prefix (`_start_idx`, `_prebins`, `_total_events` etc.). | `iv.rs:428,1122`, `solver/model.rs:133-135` |
| Q-L3 | Magic number `10` for progress bar update frequency. Should be named constant. | `iv.rs:772` |
| Q-L4 | Silent column exclusion via `filter_map` + `.ok()` in correlation -- no warning for cast failures. | `correlation.rs:50-57` |
| Q-L5 | `eprintln!` for weight warnings instead of styled output consistent with rest of UI. | `weights.rs:82` |

### Positive

- Zero unsafe code blocks in the entire codebase
- Zero `expect()` calls in production code
- Consistent `?` operator and `anyhow::Result` usage
- Strong iterator/functional patterns throughout
- No suppressed correctness clippy lints -- only style lints suppressed
- Well-structured Rayon parallelization with proper `Arc` sharing

---

## 3. Security

### High

| ID | Finding | Location |
|----|---------|----------|
| S-H1 | **Uncontrolled memory allocation via crafted SAS7BDAT headers (OOM DoS).** `page_size` (u32 max = ~4GB) and `row_count * columns` used directly for `Vec::with_capacity`. | `sas7bdat/mod.rs:87,155`, `header.rs:90-97` |

### Medium

| ID | Finding | Location |
|----|---------|----------|
| S-M1 | **Missing bounds validation on threshold CLI parameters.** `missing_threshold`, `correlation_threshold`, `gini_threshold` lack `value_parser` validators unlike `solver_gap`. | `args.rs:53-62` |
| S-M2 | **Integer truncation (u64 -> usize)** in SAS subheader pointer arithmetic. Silent truncation on 32-bit systems. Bounds check at line 197 catches it but error message is misleading. | `subheader.rs:195-196` |
| S-M3 | **Large `row_length` passed to decompression** without sanity bounds. `Vec::with_capacity(output_length)` could cause OOM. | `sas7bdat/mod.rs:209`, `decompress.rs:37,297` |
| S-M4 | **Division by zero risk** in `find_best_split` if `total_weight == 0.0` (all-zero weights vector). | `iv.rs:255-256` |

### Low

| ID | Finding | Location |
|----|---------|----------|
| S-L1 | No path canonicalization on file I/O. Output path derived from input with `../` could write to unexpected locations. Standard for CLI tools. | `main.rs`, `args.rs` |
| S-L2 | Internal details (byte offsets, page indices) in error messages included in JSON reports. | `sas7bdat/error.rs` |
| S-L3 | No file locking on outputs. Concurrent instances could overwrite. Standard for CLI tools. | `main.rs:810`, `convert.rs:194` |
| S-L4 | Direct array indexing in subheader helpers without bounds checks. Callers verify bounds but a malformed file could trigger panics. | `subheader.rs:480-530` |

### Positive

- **No unsafe code** anywhere -- eliminates buffer overflows, use-after-free, null dereferences
- **No injection vulnerabilities** -- clap parsing is safe, no shell execution of user input
- **Decompression code is well-hardened** -- output capped, input bounds checked, back-references validated
- **No temp file usage** -- all I/O goes to user-specified or derived paths

---

## 4. Performance

### Critical (Algorithmic)

| ID | Finding | Location |
|----|---------|----------|
| P-C1 | **MIP solver O(n^4) variable count** with large prebins. At prebins=100: 5050 variables, 171k constraints per feature. Auto monotonicity solves 5 MIPs per feature. | `solver/model.rs:150-241` |
| P-C2 | **`find_woe_for_value` linear scan O(B) per sample.** 100K samples * 10 bins = 1M comparisons. Binary search would reduce to O(log B). | `iv.rs:1639-1652` |

### Memory Efficiency

| ID | Finding | Location |
|----|---------|----------|
| P-M1 | **CSV loader reads entire file into memory twice** (raw bytes + parsed DataFrame). A 2GB CSV requires ~4GB+ RAM. Should use Polars native file handle or `LazyCsvReader`. | `loader.rs:67-98` |
| P-M2 | **SAS7BDAT `ColumnValue::Clone` per row per column.** 100K rows * 50 string columns = 5M heap allocations. Could use `std::mem::take`. | `sas7bdat/mod.rs:233-238` |
| P-M3 | **`weights.to_vec()` copy for Arc wrapping** -- 8MB allocation per pipeline stage for 1M-row dataset. | `iv.rs:749`, `correlation.rs:81` |

### Minor Optimization

| ID | Finding | Location |
|----|---------|----------|
| P-O1 | Sequential numeric + categorical IV processing. Could run concurrently. | `iv.rs:755-808` |
| P-O2 | Missing analysis uses `AnyValue` iteration instead of `is_null()` bitmap operations. | `missing.rs:42-47` |
| P-O3 | Greedy merge calls `merge_two_bins()` twice per iteration (search + actual merge). Cache the result. | `iv.rs:1480-1513` |
| P-O4 | SAS7BDAT two-pass page reading could be single-pass for metadata-then-data layouts. | `sas7bdat/mod.rs:90-125,148-288` |
| P-O5 | RDC byte-by-byte back-reference copy. Non-overlapping cases could use bulk `copy_within`. | `decompress.rs:600-604` |

### Positive

- Effective Rayon parallelization for IV and correlation analysis
- Smart auto-selection of correlation method (pairwise vs matrix at column threshold 15)
- Well-structured benchmarks with deterministic seeds and multiple dataset sizes
- Efficient precompute matrix for solver integration

---

## 5. Error Handling

### Critical

| ID | Finding | Location |
|----|---------|----------|
| E-C1 | **Terminal not restored on wizard error path.** `run_wizard_loop` error via `?` skips `teardown_terminal()`, leaving terminal in raw mode. | `wizard.rs:551-561` |
| E-C2 | **No panic hook in `config_menu.rs` TUI functions.** Panics during TUI leave terminal broken. Wizard correctly has a panic hook, but config_menu does not. | `config_menu.rs:134-147,161-172` |
| E-C3 | **Integer underflow in `ReductionSummary`.** `self.final_features -= features.len()` panics in debug / wraps in release if more features dropped than remain. | `summary.rs:34,39,44` |

### High

| ID | Finding | Location |
|----|---------|----------|
| E-H1 | **Silently swallowed IV analysis errors.** `result.ok()` in `filter_map` drops ALL errors (not just expected "all null"). User gets no indication features were skipped. | `iv.rs:776,806` |
| E-H2 | **Silently swallowed correlation column cast errors.** Columns that fail `cast(&DataType::Float64)` are silently excluded. | `correlation.rs:53-55` |
| E-H3 | **`compute_correlation_matrix_fast` returns `Option` not `Result`.** Multiple `return None` points with no diagnostic info. Caller gets "Failed to compute" with no reason. | `correlation.rs:394-395` |
| E-H4 | **SasError source chain lost** when converted to anyhow via `format!` Display. Should use `anyhow::Error::from()`. | `loader.rs:37,160`, `convert.rs:519` |

### Medium

| ID | Finding | Location |
|----|---------|----------|
| E-M1 | Report file cleanup after zipping silently ignores deletion failures via `.ok()`. | `reduction_report.rs:711-713` |
| E-M2 | `unreachable!()` in SAS decompressor/compression dispatch could panic on malformed files instead of returning errors. | `decompress.rs:439`, `sas7bdat/mod.rs:223` |
| E-M3 | Empty dataset not validated early after loading. Zero-row data could cascade into confusing downstream errors. | `main.rs:102-103` |
| E-M4 | `BinningStrategy::parse()` and `MonotonicityConstraint::parse()` return `String` errors requiring awkward `map_err` at every call site. | `main.rs:112,647,654` |

### Positive

- No `todo!()` or `unimplemented!()` in production code
- Production code avoids `.unwrap()` (only on infallible progress bar templates)
- Comprehensive weight/target validation
- Good `anyhow::with_context()` usage throughout
- SasError enum covers 9 well-designed variants

---

## 6. Test Coverage

### Critical Coverage Gaps

| ID | Untested Function/Module | Location |
|----|--------------------------|----------|
| T-C1 | **`get_low_gini_features()`** -- core pipeline decision function, zero tests | `iv.rs:1723` |
| T-C2 | **`find_correlated_pairs_auto()`** -- dispatch function used by pipeline, zero tests | `correlation.rs:420` |
| T-C3 | **All report export functions** -- `export_reduction_report()`, `export_reduction_report_csv()`, `package_reduction_reports()`, `export_gini_analysis_enhanced()`, `export_gini_analysis()` | `reduction_report.rs:527,547,669`, `gini_export.rs:100,185` |
| T-C4 | **`ReductionSummary`** -- all public methods untested | `summary.rs` |
| T-C5 | **Parquet-to-CSV conversion** -- only CSV-to-Parquet tested | `convert.rs` |
| T-C6 | **`count_mapped_records()`** -- used in pipeline, zero tests | `target.rs:222` |

### Missing Edge Case Tests

| ID | Missing Test | Area |
|----|-------------|------|
| T-E1 | Constant columns (zero variance) through correlation analysis | `correlation.rs` |
| T-E2 | Single-value features through IV pipeline | `iv.rs` |
| T-E3 | All-zero weights vector | `missing.rs`, `iv.rs` |
| T-E4 | Out-of-range CLI thresholds (negative, >1.0) | `args.rs` |
| T-E5 | Empty DataFrame in correlation analysis | `correlation.rs` |
| T-E6 | SAS7BDAT integration test with real fixture files | `sas7bdat/` |

### Missing Negative/Error Tests

| ID | Missing Test | Area |
|----|-------------|------|
| T-N1 | Non-existent target column in `analyze_features_iv()` | `iv.rs` |
| T-N2 | Single-class target (all 0s or all 1s) | `iv.rs` |
| T-N3 | Malformed CSV (mismatched column counts) | `loader.rs` |
| T-N4 | Truncated/corrupt Parquet file | `loader.rs` |
| T-N5 | Solver timeout behavior | `solver/` |

### Test Quality Issues

| ID | Finding | Location |
|----|---------|----------|
| T-Q1 | Most test DataFrames have 10-20 rows. With `DEFAULT_PREBINS=20`, many bins are empty. Need 100+ row tests. | `tests/common/mod.rs` |
| T-Q2 | `test_pipeline_large_dataset()` discards result with `let _ = ...` -- never asserts anything. | `test_pipeline.rs:230` |
| T-Q3 | Non-deterministic `rand::thread_rng()` in `create_large_test_dataframe()`. Should use seeded RNG. | `tests/common/mod.rs` |
| T-Q4 | No TUI rendering tests (known limitation). All visual rendering untested. | `config_menu.rs`, `wizard.rs` |

### Positive

- Excellent SAS7BDAT parser test coverage (16 decompression tests, 12 data extraction tests)
- Strong boundary testing for wizard validation (NaN, Infinity, negative zero)
- Good CLI argument test coverage (18 tests)
- Well-structured benchmarks with deterministic seeds
- Clean test helpers in `common/mod.rs`
- Integration tests cover full pipeline flow and CSV/Parquet equivalence

---

## Recommended Action Plan

### Phase 1: Critical Fixes (Safety)

1. [ ] **Fix terminal restoration** -- Add Drop guard or explicit cleanup on error paths in `wizard.rs` and `config_menu.rs`
2. [ ] **Add panic hooks to all TUI entry points** -- Extract pattern from `wizard.rs:502-507` into shared helper
3. [ ] **Replace `std::process::exit(0)` with normal control flow** -- Return `Ok(())` or `UserCancelled` variant
4. [ ] **Guard integer underflow in `ReductionSummary`** -- Use `saturating_sub`
5. [ ] **Add SAS7BDAT header sanity bounds** -- Cap `page_size`, `row_count`, `row_length` before allocation

### Phase 2: High-Impact Improvements

6. [ ] **Add CLI threshold validation** -- `value_parser` constraints for [0.0, 1.0] on missing/gini/correlation thresholds
7. [ ] **Log swallowed IV/correlation errors** -- Separate expected failures from genuine errors, warn on skipped features
8. [ ] **Unify Config/PipelineConfig** -- Single struct or `TryFrom` conversion
9. [ ] **Eliminate unnecessary DataFrame clones** -- Refactor `drop_many` usage to avoid cloning
10. [ ] **Convert `compute_correlation_matrix_fast` to return `Result`** -- Better diagnostics on failure

### Phase 3: Test Coverage

11. [ ] **Add tests for `get_low_gini_features()`** -- Basic filtering, boundary, empty input
12. [ ] **Add tests for `find_correlated_pairs_auto()`** -- Dispatch logic, threshold behavior
13. [ ] **Add report export tests** -- JSON, CSV, ZIP packaging with temp dirs
14. [ ] **Add Parquet-to-CSV conversion tests**
15. [ ] **Fix non-deterministic seed** in `create_large_test_dataframe()`
16. [ ] **Add constant-column and all-zero-weights edge case tests**

### Phase 4: Maintenance & Performance

17. [ ] **Split `iv.rs`** into sub-modules (types, cart, quantile, woe, categorical)
18. [ ] **Extract shared TUI rendering helpers** from config_menu.rs and wizard.rs
19. [ ] **Extract path derivation helper** to eliminate 7x duplication
20. [ ] **Binary search in `find_woe_for_value`** for O(log B) instead of O(B)
21. [ ] **Remove unused `dialoguer` dependency**
22. [ ] **Audit and reduce `#[allow(dead_code)]` suppressions**
23. [ ] **Use `BTreeMap` or sorted Vec** for deterministic categorical merging in IV analysis
24. [ ] **Preserve SasError chain** when converting to anyhow

---

## Codebase Health Scorecard

| Dimension | Score | Notes |
|-----------|-------|-------|
| **Architecture** | 7/10 | Clean pipeline, good module boundaries. Config duplication and iv.rs monolith are main concerns. |
| **Code Quality** | 8/10 | Zero unsafe, zero production expect(), strong idioms. Minor anti-patterns. |
| **Security** | 7/10 | Rust safety eliminates most classes. SAS7BDAT OOM and missing validation are actionable. |
| **Performance** | 7/10 | Good parallelism, smart algorithm selection. CSV double-buffering and solver scaling are concerns. |
| **Error Handling** | 6/10 | Terminal restoration bug is critical. Silently swallowed errors in core pipeline. |
| **Test Coverage** | 6/10 | Strong unit tests for SAS parser and pipeline. Missing tests for key decision functions and exports. |
| **Overall** | **7/10** | Solid foundation with clear improvement paths. Phase 1 fixes should be prioritized before release. |
