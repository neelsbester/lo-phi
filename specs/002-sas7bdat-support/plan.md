# Implementation Plan: SAS7BDAT File Format Support

**Spec Version:** 1.0.0
**Date:** 2026-02-01

---

## Constitution Check

- [x] **Principle 1 (Statistical Correctness):** IEEE 754 doubles are reconstructed bit-identically from truncated (3-8 byte) SAS storage. Missing value sentinels are detected and mapped to `null`. Date/datetime epoch conversion uses exact integer constants (3,653 days / 315,619,200 seconds). No rounding or approximation anywhere in the parser.

- [x] **Principle 2 (Performance):** Page-by-page streaming reads — never loads the entire file into memory. Row data is extracted per-page and accumulated into column-oriented Polars Series. Decompression is applied per-page, not per-file. Progress bar shows `pages parsed / total pages` for user feedback on large files.

- [x] **Principle 3 (Transparency):** The parser is a data loader — it feeds the existing pipeline which already handles all reporting and traceability. Parse warnings (unknown encoding, skipped AMD pages) are logged to stderr. Errors include byte offsets where possible (NFR-4).

- [x] **Principle 4 (Ergonomic UX):** `.sas7bdat` files appear in the TUI file selector. The `[F]` key converts SAS7BDAT→Parquet. Output defaults to Parquet when input is SAS7BDAT (since write-back is not supported). No new CLI flags required — existing `--input` accepts the new extension.

- [x] **Principle 5 (Testing):** Known-answer tests against reference SAS7BDAT files (generated via SAS or pandas). Unit tests for header parsing, each decompression algorithm, date conversion, missing values, truncated numerics, encoding conversion. Integration test for full pipeline with SAS7BDAT input. Edge cases: zero-row file, truncated file, corrupted magic number, big-endian file, 32-bit file.

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                         User Entry Points                        │
│  CLI: --input data.sas7bdat    TUI: file selector + [F] convert │
└──────────────────┬──────────────────────────────┬────────────────┘
                   │                              │
                   v                              v
┌──────────────────────────┐    ┌─────────────────────────────────┐
│  src/pipeline/loader.rs  │    │  src/cli/convert.rs             │
│  load_dataset_with_      │    │  run_convert() — extended to    │
│  progress() — new match  │    │  accept .sas7bdat input and     │
│  arm for "sas7bdat"      │    │  convert to Parquet/CSV         │
└──────────┬───────────────┘    └──────────┬──────────────────────┘
           │                               │
           v                               v
┌──────────────────────────────────────────────────────────────────┐
│              src/pipeline/sas7bdat/mod.rs                        │
│  Public API:                                                     │
│    load_sas7bdat(path) -> Result<(DataFrame, usize, usize, f64)>│
│    get_sas7bdat_columns(path) -> Result<Vec<String>>             │
└──────────┬───────────────────────────────────────────────────────┘
           │
           v
┌──────────────────────────────────────────────────────────────────┐
│  Internal Parser Modules                                         │
│  ┌─────────────┐ ┌──────────┐ ┌─────────────┐ ┌──────────────┐ │
│  │ header.rs   │→│ page.rs  │→│subheader.rs │→│  column.rs   │ │
│  │ Magic num,  │ │ Page iter│ │ Row size,   │ │ Names,types, │ │
│  │ alignment,  │ │ Type     │ │ Col size,   │ │ formats,     │ │
│  │ endianness, │ │ dispatch │ │ Col text,   │ │ labels       │ │
│  │ encoding    │ │          │ │ Col attrs   │ │              │ │
│  └─────────────┘ └──────────┘ └─────────────┘ └──────────────┘ │
│  ┌──────────────┐ ┌──────────┐ ┌──────────────┐                │
│  │decompress.rs │ │ data.rs  │ │constants.rs  │                │
│  │ RLE, RDC     │ │ Row      │ │ Magic, sigs, │                │
│  │ algorithms   │ │ extract, │ │ offsets,     │                │
│  │              │ │ type     │ │ encoding map │                │
│  │              │ │ convert  │ │              │                │
│  └──────────────┘ └──────────┘ └──────────────┘                │
│  ┌──────────────┐                                               │
│  │  error.rs    │  SasError enum with byte offsets              │
│  └──────────────┘                                               │
└──────────────────────────────────────────────────────────────────┘
           │
           v
┌──────────────────────────────────────────────────────────────────┐
│                  Polars DataFrame                                 │
│  Numeric → Float64 | String → Utf8 | Date → Date | DT → Datetime│
└──────────────────────────────────────────────────────────────────┘
           │
           v
┌──────────────────────────────────────────────────────────────────┐
│           Existing Pipeline (unchanged)                           │
│  Missing Analysis → Gini/IV Analysis → Correlation Analysis      │
└──────────────────────────────────────────────────────────────────┘
```

### Phase Mapping

The implementation steps below use 5 phases. The corresponding `tasks.md` decomposes these into 7 finer-grained phases:

| Plan Phase | Tasks Phase(s) |
|-----------|---------------|
| Phase 1: Core Parser Foundation | Phase 1 (Setup) + Phase 2 (Foundational) + Phase 3/TASK-04 through TASK-09 |
| Phase 2: Data Extraction and Decompression | Phase 3/TASK-08 through TASK-10 |
| Phase 3: Integration | Phase 3/TASK-11-12 + Phase 4 (US2) + Phase 5 (US3) |
| Phase 4: Testing | Phase 6 (Testing) + Phase 6b (Benchmarks) |
| Phase 5: Documentation and Finalization | Phase 7 (Polish) |

### Key Design Decisions (from research.md)

1. **Custom pure Rust parser** — no viable crates exist; `readstat-rs` uses C FFI (violates NFR-1) and rounds floats (violates NFR-3)
2. **`encoding_rs` crate** for character encoding conversion (UTF-8, Latin-1, Windows-1252, CJK)
3. **Page-by-page streaming** — reads one page at a time, never buffers entire file
4. **Module-per-concern architecture** — 9 files in `src/pipeline/sas7bdat/`

---

## Implementation Steps

### Phase 1: Core Parser Foundation

1. **Create module structure and constants**
   - Files: `src/pipeline/sas7bdat/mod.rs`, `constants.rs`, `error.rs`
   - Dependencies: None
   - Define: magic number, subheader signatures (32/64-bit), page type values, encoding ID→name table, all header field offsets, SAS epoch constants
   - Define: `SasError` enum with variants for each failure mode (InvalidMagic, UnsupportedCompression, TruncatedFile, UnsupportedEncoding, InvalidPageType, etc.)

2. **Implement header parsing**
   - Files: `src/pipeline/sas7bdat/header.rs`
   - Dependencies: Step 1
   - Parse: magic number validation, 32/64-bit alignment detection, endianness, encoding, page size, page count, row count, row length, column count, dataset name, timestamps
   - Return: `SasHeader` struct with all parsed metadata
   - Edge cases: big-endian files, 32-bit files, invalid magic number, truncated header

3. **Implement page iteration**
   - Files: `src/pipeline/sas7bdat/page.rs`
   - Dependencies: Step 2
   - Read pages sequentially from file after header
   - Parse page header: type, block count, subheader pointer count
   - Dispatch by page type: Meta → subheader processing, Data → row extraction, Mix → both, AMD → skip with warning
   - Yield page data for further processing

4. **Implement subheader parsing**
   - Files: `src/pipeline/sas7bdat/subheader.rs`
   - Dependencies: Steps 1, 3
   - Parse subheader pointers (offset, length, compression flag, type)
   - Dispatch by signature to handlers:
     - Row Size → extract row_length, total_row_count, max_rows_on_mix_page
     - Column Size → extract column_count
     - Column Text → accumulate text blocks
     - Column Name → extract names from text blocks
     - Column Attributes → extract type (numeric/char), offset, length per column
     - Format and Label → extract SAS format string per column
     - Column List, Subheader Counts → metadata

5. **Implement column metadata extraction**
   - Files: `src/pipeline/sas7bdat/column.rs`
   - Dependencies: Step 4
   - Combine subheader data into `SasColumn` structs: name, data_type (Numeric/Character), offset, length, format_name, label
   - Detect date/datetime columns from format string (case-insensitive, strip width: `DATE9.` → `DATE`)
   - Supported date formats: DATE, DATETIME, TIME, DDMMYY, MMDDYY, YYMMDD

### Phase 2: Data Extraction and Decompression

6. **Implement RLE decompression**
   - Files: `src/pipeline/sas7bdat/decompress.rs`
   - Dependencies: Step 1
   - Implement all 16 control byte commands per ReadStat reference
   - Input: compressed byte slice + expected output length
   - Output: decompressed byte vector
   - Error on: buffer overflow, premature input exhaustion

7. **Implement RDC decompression**
   - Files: `src/pipeline/sas7bdat/decompress.rs` (same file)
   - Dependencies: Step 1
   - Implement 16-bit control word + 4 command types per Parso reference
   - Sliding window back-references for pattern matching
   - Input: compressed byte slice + expected output length
   - Output: decompressed byte vector

8. **Implement row data extraction and type conversion**
   - Files: `src/pipeline/sas7bdat/data.rs`
   - Dependencies: Steps 5, 6, 7
   - Extract rows from data pages (and data portion of mix pages)
   - For each row, for each column:
     - **Numeric**: Read N bytes (3-8), reconstruct IEEE 754 double via zero-padding MSB, detect missing value sentinels → `null`
     - **Character**: Read fixed-width bytes, decode via `encoding_rs` to UTF-8, trim trailing spaces
     - **Date**: Numeric extraction + epoch conversion (`sas_days - 3653`)
     - **Datetime**: Numeric extraction + epoch conversion (`(sas_seconds - 315_619_200) * 1000`)
     - **Time**: Numeric extraction → nanoseconds since midnight
   - Decompress page data if compression detected (RLE or RDC)

9. **Implement public API and DataFrame assembly**
   - Files: `src/pipeline/sas7bdat/mod.rs`
   - Dependencies: Steps 2-8
   - `load_sas7bdat(path) -> Result<(DataFrame, usize, usize, f64)>`:
     1. Parse header
     2. Validate: non-zero row count, supported encoding
     3. Extract column metadata (names, types, formats)
     4. Iterate pages, extract rows, accumulate into per-column vectors
     5. Build Polars Series from vectors (Float64, Utf8, Date, Datetime, Time)
     6. Assemble DataFrame, compute memory estimate
     7. Return (df, rows, cols, memory_mb)
   - `get_sas7bdat_columns(path) -> Result<Vec<String>>`:
     1. Parse header + metadata pages only (stop at first data page)
     2. Return column names
   - Progress bar: pages processed / total pages (via indicatif, consistent with existing loader)

### Phase 3: Integration

10. **Integrate with loader module**
    - Files: `src/pipeline/loader.rs`, `src/pipeline/mod.rs`
    - Dependencies: Step 9
    - Add `"sas7bdat"` arm to `get_column_names()` match statement
    - Add `"sas7bdat"` arm to `load_dataset_with_progress()` match statement
    - Update error message for unsupported formats to list sas7bdat
    - Add `pub mod sas7bdat;` to `src/pipeline/mod.rs`

11. **Integrate with TUI file selector**
    - Files: `src/cli/config_menu.rs`
    - Dependencies: Step 10
    - Update `is_valid_data_file()` to include `e.eq_ignore_ascii_case("sas7bdat")`

12. **Integrate with CLI args and help text**
    - Files: `src/cli/args.rs`
    - Dependencies: Step 10
    - Update `--input` help text: "Input dataset file (CSV, Parquet, or SAS7BDAT)"
    - Update `--output` help text to mention SAS7BDAT input defaults to Parquet

13. **Integrate with convert subcommand**
    - Files: `src/cli/convert.rs`
    - Dependencies: Step 9
    - Add SAS7BDAT→Parquet conversion path:
      - Detect `.sas7bdat` extension on input
      - Load via `load_sas7bdat()` from Step 9
      - Write to Parquet using existing `ParquetWriter`
    - Support SAS7BDAT→CSV conversion if `--output` specifies `.csv`

14. **Handle output format defaulting**
    - Files: `src/main.rs`
    - Dependencies: Step 10
    - In `output_path()` derivation: when input extension is `sas7bdat`, default output to `.parquet` instead of preserving input extension
    - Ensure `save_dataset()` does not need changes (already handles csv/parquet output)

15. **Update TUI conversion flow**
    - Files: `src/cli/config_menu.rs`
    - Dependencies: Step 13
    - Update `[F]` key handler to support SAS7BDAT→Parquet conversion
    - Display appropriate conversion message ("Converting SAS7BDAT to Parquet...")

### Phase 4: Testing

16. **Create test reference files**
    - Files: `tests/fixtures/` (new directory for binary test files)
    - Dependencies: Step 9
    - Generate reference SAS7BDAT files using Python/pandas:
      - `basic_64bit_le.sas7bdat` — standard 64-bit little-endian, mixed types
      - `basic_32bit.sas7bdat` — 32-bit format
      - `compressed_rle.sas7bdat` — RLE compressed (COMPRESS=CHAR)
      - `compressed_rdc.sas7bdat` — RDC compressed (COMPRESS=BINARY)
      - `with_dates.sas7bdat` — DATE, DATETIME, TIME columns
      - `with_missing.sas7bdat` — standard and special missing values
      - `latin1_encoding.sas7bdat` — Latin-1 encoded strings
      - `truncated_numerics.sas7bdat` — columns with LENGTH < 8
      - `zero_rows.sas7bdat` — empty dataset (header only)
      - `truncated_file.sas7bdat` — file cut short mid-page
    - Also generate expected-output CSV/Parquet for value comparison

17. **Unit tests for parser modules**
    - Files: `src/pipeline/sas7bdat/*.rs` (inline `#[cfg(test)]` modules)
    - Dependencies: Step 16
    - Tests per module:
      - `constants.rs`: magic number length, encoding table completeness
      - `header.rs`: 64-bit LE header parse, 32-bit header parse, big-endian detection, invalid magic, truncated header
      - `decompress.rs`: RLE round-trip on known data, all 16 RLE commands, RDC round-trip, empty input, buffer overflow protection
      - `data.rs`: truncated numeric reconstruction (3,4,5,6,7,8 bytes), missing value sentinel detection (`.`, `.A`-`.Z`), date/datetime epoch conversion, character encoding (Latin-1, UTF-8)
      - `column.rs`: format string parsing (DATE9. → Date, DATETIME20. → Datetime, BEST12. → Float64)

18. **Integration tests**
    - Files: `tests/test_sas7bdat.rs` (new)
    - Dependencies: Steps 10, 16
    - Tests:
      - `test_load_sas7bdat_basic` — load and verify shape, column names, sample values
      - `test_load_sas7bdat_compressed_rle` — load RLE-compressed file
      - `test_load_sas7bdat_compressed_rdc` — load RDC-compressed file
      - `test_load_sas7bdat_dates` — verify date/datetime conversion correctness
      - `test_load_sas7bdat_missing_values` — verify nulls in expected positions
      - `test_load_sas7bdat_encoding_latin1` — verify Latin-1 string decoding
      - `test_get_column_names_sas7bdat` — metadata-only extraction
      - `test_sas7bdat_zero_rows_rejected` — clear error for empty files
      - `test_sas7bdat_truncated_file` — clear error for corrupted files
      - `test_sas7bdat_invalid_magic` — clear error for non-SAS files
      - `test_full_pipeline_sas7bdat` — end-to-end: load → missing → IV → correlation → save
      - `test_convert_sas7bdat_to_parquet` — conversion produces valid Parquet
      - `test_convert_sas7bdat_to_csv` — conversion produces valid CSV

19. **Add test fixture helpers to common module**
    - Files: `tests/common/mod.rs`
    - Dependencies: Step 16
    - Add `create_temp_sas7bdat()` helper (copies fixture file to temp dir)
    - Add `sas7bdat_fixture_path(name)` helper to locate test fixtures

### Phase 5: Documentation and Finalization

20. **Add `encoding_rs` dependency** *(Note: This step must execute before Phase 1. It is listed here for document completeness but is Task T001 in the implementation sequence.)*
    - Files: `Cargo.toml`
    - Dependencies: None (do first — Phase 1 prerequisite)
    - Add `encoding_rs = "0.8"` to `[dependencies]`

21. **Update CLAUDE.md**
    - Files: `CLAUDE.md`
    - Dependencies: Steps 10-15
    - Add SAS7BDAT to module structure documentation
    - Update loader description to include SAS7BDAT
    - Update TUI file selector docs
    - Add SAS7BDAT to output files section (default to Parquet)
    - Update key dependencies to include `encoding_rs`

22. **Update project documentation**
    - Files: `docs/user-guide.md`, `docs/architecture.md`
    - Dependencies: Steps 10-15
    - Update user guide: SAS7BDAT as accepted input, conversion options
    - Update architecture: new sas7bdat module in module structure

23. **Create ADR-009: SAS7BDAT Pure Rust Parser**
    - Files: `docs/adr/009-sas7bdat-pure-rust-parser.md` (new)
    - Dependencies: Steps 10-15
    - Document: context (need for SAS support), decision (custom pure Rust parser), alternatives (readstat-rs, quick-sas7bdat, Python shelling), consequences

---

## Testing Strategy

- **Unit tests:** Per-module `#[cfg(test)]` blocks testing individual parsing functions against known byte sequences. Focus areas: header parsing edge cases, all 16 RLE commands, RDC sliding window, truncated numeric reconstruction (3-8 bytes), missing value sentinel detection, date format string matching, encoding conversion.

- **Integration tests:** End-to-end tests in `tests/test_sas7bdat.rs` using reference fixture files. Verify: data shapes, column types, numeric values (bit-exact comparison), date values, string values, null positions. Include full pipeline test (load → analyze → save).

- **Regression tests:** Existing CSV/Parquet test suite must continue passing (`cargo test --all-features`). No changes to existing analysis modules.

- **Benchmarks:** Add `benches/sas7bdat_benchmark.rs` for page parsing throughput on fixture files, comparing RLE vs RDC vs uncompressed parsing speed. Required by Constitution Principle 2 ("Performance regressions MUST be detected via Criterion benchmarks").

---

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Format misinterpretation in edge cases (rare page types, unusual subheader ordering) | High — silent data corruption | Cross-reference 3+ implementations (pandas, Parso, ReadStat); extensive known-answer testing |
| RDC decompression bugs (less-documented algorithm) | Medium — fails on COMPRESS=BINARY files | Use Parso Java source as primary reference; test with pandas-generated RDC files |
| Big-endian SAS files not tested thoroughly | Low — very rare in practice | Implement correctly but note as lower-priority test path per spec assumptions |
| Large file memory pressure (multi-GB files) | Medium — OOM on constrained systems | Page-by-page streaming; accumulate into column vectors not row-by-row DataFrames; document memory behavior |
| Exotic character encodings not handled | Low — most files are UTF-8/Latin-1 | `encoding_rs` covers all WHATWG encodings; unsupported encodings produce clear error (not garbled data) |
| Truncated numeric reconstruction errors | High — silent precision loss | Implement known-answer tests for each byte width (3-8); compare against pandas output |
| SAS missing value sentinel not detected | High — missing values appear as near-zero floats | Test all 28 sentinel patterns (`.`, `.A`-`.Z`, `._`); verify null count matches expected |

---

## Rollback Plan

The SAS7BDAT parser is entirely additive:
- New module: `src/pipeline/sas7bdat/` — delete the directory
- Loader changes: Remove match arm in `loader.rs` (2 locations)
- TUI changes: Remove `"sas7bdat"` from `is_valid_data_file()` (1 line)
- CLI changes: Revert help text in `args.rs` (2 lines)
- Convert changes: Remove SAS7BDAT path in `convert.rs`
- Main changes: Revert output extension logic in `main.rs`
- Dependency: Remove `encoding_rs` from `Cargo.toml`

No existing code paths are modified — all changes are additive match arms or new modules. Reverting is a clean deletion with no risk to existing functionality.

---

## Generated Artifacts

| Artifact | Path | Status |
|----------|------|--------|
| Feature Spec | `specs/002-sas7bdat-support/spec.md` | Complete |
| Research | `specs/002-sas7bdat-support/research.md` | Complete |
| Data Model | `specs/002-sas7bdat-support/data-model.md` | Complete |
| Quickstart | `specs/002-sas7bdat-support/quickstart.md` | Complete |
| Implementation Plan | `specs/002-sas7bdat-support/plan.md` | This document |
