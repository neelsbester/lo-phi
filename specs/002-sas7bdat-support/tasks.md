# Tasks: SAS7BDAT File Format Support

**Plan Version:** 1.0.0
**Date:** 2026-02-01

---

## Task Categories

Tasks are categorized by the constitution principle they primarily serve:

- **STAT:** Statistical Correctness (Principle 1)
- **PERF:** Performance at Scale (Principle 2)
- **TRANS:** Transparent Decision-Making (Principle 3)
- **UX:** Ergonomic TUI/CLI (Principle 4)
- **TEST:** Rigorous Testing (Principle 5)

---

## ID Convention

This file uses a dual-ID system:
- **TASK-XX** (e.g., TASK-01): Parent task containers grouping related work. Depending on a TASK-XX means **all** its child tasks must be complete.
- **T0XX** (e.g., T004): Individual checklist items within a parent task. These are the atomic units of work.
- **[P]**: Task can be executed in parallel with other [P]-marked siblings in the same parent.
- **[USn]**: Task belongs to User Story n.

---

## Phase 1: Setup

Goal: Initialize module structure and add dependencies.

- [ ] T001 Add `encoding_rs = "0.8"` dependency to `Cargo.toml`
- [ ] T002 Create module directory structure `src/pipeline/sas7bdat/` with files: `mod.rs`, `constants.rs`, `error.rs`, `header.rs`, `page.rs`, `subheader.rs`, `column.rs`, `decompress.rs`, `data.rs`
- [ ] T003 Add `pub mod sas7bdat;` to `src/pipeline/mod.rs`

---

## Phase 2: Foundational — Core Parser Types and Constants

Goal: Establish all shared types, constants, and error definitions that every subsequent task depends on. These must be complete before any user story work begins.

### TASK-01: Define SAS7BDAT Constants

- **Category:** STAT
- **Priority:** P0
- **Dependencies:** T001, T002, T003
- **Files:** `src/pipeline/sas7bdat/constants.rs`
- **Description:** Define all binary format constants from the SAS7BDAT specification. This is the foundational module imported by every other parser module.
- **Acceptance Criteria:**
  - [ ] T004 Define 32-byte magic number constant `SAS_MAGIC` in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T005 [P] Define alignment detection constants (offset 32, value `0x33` for 64-bit) in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T006 [P] Define endianness detection constants (offset 37, `0x01` = LE, `0x00` = BE) in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T007 [P] Define all header field offsets (encoding at 70, timestamps, page size at 200+a1, page count at 204+a1, etc.) in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T008 [P] Define subheader signature constants for both 32-bit and 64-bit variants (RowSize, ColumnSize, SubheaderCounts, ColumnText, ColumnName, ColumnAttributes, FormatAndLabel, ColumnList) in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T009 [P] Define page type constants (Meta=0x0000, Data=0x0100, Mix=0x0200, AMD=0x0400, Meta2=0x4000, Comp=0x9000) in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T010 [P] Define compression identifier constants (`SASYZCRL` for RLE, `SASYZCR2` for RDC) in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T011 [P] Define encoding ID→name mapping table (20=UTF-8, 28=ASCII, 29=Latin-1, 62=Windows-1252, 125=EUC-CN, 134=EUC-JP, 138=Shift-JIS, 140=EUC-KR) in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T012 [P] Define SAS epoch conversion constants (`SAS_EPOCH_OFFSET_DAYS = 3653`, `SAS_EPOCH_OFFSET_SECONDS = 315_619_200`, `MS_PER_SECOND = 1000`, `NS_PER_SECOND = 1_000_000_000`) in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T013 [P] Define SAS date format patterns (DATE, DATETIME, TIME, DDMMYY, MMDDYY, YYMMDD) for format detection in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T014 [P] Define SAS missing value sentinel byte patterns (`.`, `.A`-`.Z`, `._` — 28 total patterns) in `src/pipeline/sas7bdat/constants.rs`
- **Status:** Pending

---

### TASK-02: Define Error Types

- **Category:** TRANS
- **Priority:** P0
- **Dependencies:** T002
- **Files:** `src/pipeline/sas7bdat/error.rs`
- **Description:** Define the `SasError` enum with all failure variants and implement `Display`, `Error`, and `From<std::io::Error>` traits. Error messages must include byte offsets where possible (NFR-4).
- **Acceptance Criteria:**
  - [ ] T015 Define `SasError` enum with variants: `InvalidMagic`, `TruncatedFile { expected, actual }`, `ZeroRows`, `UnsupportedEncoding { id }`, `InvalidPageType { page_index, page_type }`, `UnknownSubheader { signature, offset }`, `DecompressionError { page_index, message }`, `NumericError { column, row, message }`, `Io(std::io::Error)` in `src/pipeline/sas7bdat/error.rs`
  - [ ] T016 Implement `std::fmt::Display` for `SasError` with human-readable messages including byte offsets in `src/pipeline/sas7bdat/error.rs`
  - [ ] T017 [P] Implement `std::error::Error` for `SasError` in `src/pipeline/sas7bdat/error.rs`
  - [ ] T018 [P] Implement `From<std::io::Error>` for `SasError` in `src/pipeline/sas7bdat/error.rs`
- **Status:** Pending

---

### TASK-03: Define Core Data Types

- **Category:** STAT
- **Priority:** P0
- **Dependencies:** T002
- **Files:** `src/pipeline/sas7bdat/mod.rs` (types section)
- **Description:** Define all shared structs and enums from the data model (`SasHeader`, `SasColumn`, `SasDataType`, `PolarsOutputType`, `Compression`, `OsType`, `SasEncoding`). These types are used across all parser modules.
- **Acceptance Criteria:**
  - [ ] T019 Define `SasDataType` enum (Numeric, Character) in `src/pipeline/sas7bdat/mod.rs`
  - [ ] T020 [P] Define `PolarsOutputType` enum (Float64, Date, Datetime, Time, Utf8) in `src/pipeline/sas7bdat/mod.rs`
  - [ ] T021 [P] Define `Compression` enum (None, Rle, Rdc) in `src/pipeline/sas7bdat/mod.rs`
  - [ ] T022 [P] Define `OsType` enum (Unix, Windows, Unknown) in `src/pipeline/sas7bdat/mod.rs`
  - [ ] T023 [P] Define `SasEncoding` enum (Utf8, Ascii, Latin1, Windows1252, Other { id, name }, Unspecified) in `src/pipeline/sas7bdat/mod.rs`
  - [ ] T024 [P] Define `SasHeader` struct with all fields per data model in `src/pipeline/sas7bdat/mod.rs`
  - [ ] T025 [P] Define `SasColumn` struct with all fields per data model in `src/pipeline/sas7bdat/mod.rs`
- **Status:** Pending

---

## Phase 3: User Story 1 — Load SAS7BDAT into Pipeline [FR-1, FR-2, FR-3, FR-4, FR-6, FR-10]

**Story Goal:** As a user, I can provide a `.sas7bdat` file as input and have it load as a Polars DataFrame for the feature reduction pipeline.

**Independent Test Criteria:** `load_sas7bdat(path)` returns a correct DataFrame with proper types, `get_sas7bdat_columns(path)` returns column names, and the full pipeline (missing → IV → correlation → save) succeeds on SAS7BDAT input.

### TASK-04: Implement Header Parsing

- **Category:** STAT
- **Priority:** P0
- **Dependencies:** TASK-01, TASK-02, TASK-03
- **Files:** `src/pipeline/sas7bdat/header.rs`
- **Description:** Parse the SAS7BDAT file header to extract all metadata. This is the entry point of the parser: validates magic number, detects alignment (32/64-bit), endianness, encoding, page layout, and row/column counts.
- **Acceptance Criteria:**
  - [ ] T026 [US1] Implement `parse_header(reader) -> Result<SasHeader, SasError>` that reads and validates the magic number in `src/pipeline/sas7bdat/header.rs`
  - [ ] T027 [US1] Parse alignment byte (offset 32) to detect 32-bit vs 64-bit format in `src/pipeline/sas7bdat/header.rs`
  - [ ] T028 [P] [US1] Parse endianness byte (offset 37) and configure byte-order reading accordingly in `src/pipeline/sas7bdat/header.rs`
  - [ ] T029 [P] [US1] Parse encoding bytes (offset 70) and map to `SasEncoding` variant. Encoding ID 0 maps to `SasEncoding::Unspecified` (falls back to Latin-1 at decode time). Unknown non-zero IDs not in the static table produce `SasError::UnsupportedEncoding`. In `src/pipeline/sas7bdat/header.rs`
  - [ ] T030 [US1] Parse page size, page count, row count, row length, column count using alignment-aware offsets in `src/pipeline/sas7bdat/header.rs`
  - [ ] T031 [P] [US1] Parse dataset name, creation/modification timestamps, SAS release version, and OS type in `src/pipeline/sas7bdat/header.rs`
  - [ ] T032 [US1] Return `SasError::InvalidMagic` for files that don't start with the magic number in `src/pipeline/sas7bdat/header.rs`
  - [ ] T033 [P] [US1] Return `SasError::TruncatedFile` when file is shorter than declared header length in `src/pipeline/sas7bdat/header.rs`
- **Status:** Pending

---

### TASK-05: Implement Page Iteration

- **Category:** STAT
- **Priority:** P0
- **Dependencies:** TASK-04
- **Files:** `src/pipeline/sas7bdat/page.rs`
- **Description:** Read pages sequentially from the file after the header. Parse page headers (type, block count, subheader pointer count) and dispatch by page type: Meta → subheader processing, Data → row extraction, Mix → both, AMD → skip with warning.
- **Acceptance Criteria:**
  - [ ] T034 [US1] Implement page header parsing (page type, block count, subheader pointer count) with alignment-aware offsets in `src/pipeline/sas7bdat/page.rs`
  - [ ] T035 [US1] Implement page type dispatch (Meta, Data, Mix, AMD, Meta2, Comp) in `src/pipeline/sas7bdat/page.rs`
  - [ ] T036 [P] [US1] Skip AMD pages with stderr warning per Principle 3 (Transparency) in `src/pipeline/sas7bdat/page.rs`
  - [ ] T037 [P] [US1] Return `SasError::InvalidPageType` for unrecognized page types in `src/pipeline/sas7bdat/page.rs`
- **Status:** Pending

---

### TASK-06: Implement Subheader Parsing

- **Category:** STAT
- **Priority:** P0
- **Dependencies:** TASK-05
- **Files:** `src/pipeline/sas7bdat/subheader.rs`
- **Description:** Parse subheader pointers from meta/mix pages and dispatch by signature to extract row size, column size, column text blocks, column names, column attributes, and format/label information.
- **Acceptance Criteria:**
  - [ ] T038 [US1] Parse subheader pointer entries (offset, length, compression flag, type) from page data in `src/pipeline/sas7bdat/subheader.rs`
  - [ ] T039 [US1] Implement signature-based dispatch to handler functions for each subheader type in `src/pipeline/sas7bdat/subheader.rs`
  - [ ] T040 [US1] Implement Row Size subheader handler: extract `row_length`, `total_row_count`, `max_rows_on_mix_page` in `src/pipeline/sas7bdat/subheader.rs`
  - [ ] T041 [P] [US1] Implement Column Size subheader handler: extract `column_count` in `src/pipeline/sas7bdat/subheader.rs`
  - [ ] T042 [US1] Implement Column Text subheader handler: accumulate text blocks in `src/pipeline/sas7bdat/subheader.rs`
  - [ ] T043 [US1] Implement Column Name subheader handler: extract names from text blocks in `src/pipeline/sas7bdat/subheader.rs`
  - [ ] T044 [US1] Implement Column Attributes subheader handler: extract type (numeric/char), offset, length per column in `src/pipeline/sas7bdat/subheader.rs`
  - [ ] T045 [US1] Implement Format and Label subheader handler: extract SAS format string per column in `src/pipeline/sas7bdat/subheader.rs`
  - [ ] T046 [P] [US1] Detect compression type from Column Text subheader (`SASYZCRL` = RLE, `SASYZCR2` = RDC) in `src/pipeline/sas7bdat/subheader.rs`
- **Status:** Pending

---

### TASK-07: Implement Column Metadata Extraction

- **Category:** STAT
- **Priority:** P0
- **Dependencies:** TASK-06
- **Files:** `src/pipeline/sas7bdat/column.rs`
- **Description:** Combine subheader data into `SasColumn` structs. Detect date/datetime columns from format strings and derive `PolarsOutputType` for each column.
- **Acceptance Criteria:**
  - [ ] T047 [US1] Implement `build_columns()` that assembles `SasColumn` structs from accumulated subheader data (names, types, offsets, lengths, formats, labels) in `src/pipeline/sas7bdat/column.rs`
  - [ ] T048 [US1] Implement format string parsing: case-insensitive match, strip width/decimal (e.g., `DATE9.` → `DATE`, `DATETIME20.` → `DATETIME`) in `src/pipeline/sas7bdat/column.rs`
  - [ ] T049 [US1] Derive `PolarsOutputType` from `SasDataType` + format: DATE/DDMMYY/MMDDYY/YYMMDD → Date, DATETIME → Datetime, TIME → Time, Character → Utf8, other Numeric → Float64 in `src/pipeline/sas7bdat/column.rs`
- **Status:** Pending

---

### TASK-08: Implement Decompression Algorithms

- **Category:** STAT
- **Priority:** P0
- **Dependencies:** TASK-01
- **Files:** `src/pipeline/sas7bdat/decompress.rs`
- **Description:** Implement both RLE and RDC decompression algorithms for compressed SAS7BDAT page data. RLE uses control byte commands (16 types per ReadStat reference). RDC uses 16-bit control word with sliding window back-references (per Parso reference).
- **Acceptance Criteria:**
  - [ ] T050 [US1] Implement RLE decompression: parse control bytes, implement all 16 commands (COPY64, COPY64_PLUS_4096, COPY96, INSERT_BYTE18, INSERT_AT17, INSERT_BLANK17, INSERT_ZERO17, COPY1, COPY17, COPY33, COPY49, INSERT_BYTE3, INSERT_AT2, INSERT_BLANK2, INSERT_ZERO2) in `src/pipeline/sas7bdat/decompress.rs`
  - [ ] T051 [US1] Implement RDC decompression: 16-bit control word, literal copy, Short RLE, Long RLE, Long Pattern (back-ref 16+), Short Pattern (back-ref 3-15) in `src/pipeline/sas7bdat/decompress.rs`
  - [ ] T052 [US1] Return `SasError::DecompressionError` on buffer overflow or premature input exhaustion in `src/pipeline/sas7bdat/decompress.rs`
- **Status:** Pending

---

### TASK-09: Implement Row Data Extraction and Type Conversion

- **Category:** STAT
- **Priority:** P0
- **Dependencies:** TASK-07, TASK-08
- **Files:** `src/pipeline/sas7bdat/data.rs`
- **Description:** Extract row data from data/mix pages, apply decompression if needed, and convert each column value to the appropriate Rust/Polars type. Handle truncated numerics (3-8 bytes), missing value sentinels, date/datetime epoch conversion, and character encoding.
- **Acceptance Criteria:**
  - [ ] T053 [US1] Implement row extraction from data pages: iterate rows using row_length, extract column slices using column offset/length in `src/pipeline/sas7bdat/data.rs`
  - [ ] T054 [US1] Implement row extraction from mix pages (data portion after subheaders) using `max_rows_on_mix_page` in `src/pipeline/sas7bdat/data.rs`
  - [ ] T055 [US1] Implement truncated numeric reconstruction: zero-pad MSB for 3-8 byte values, handle endianness in `src/pipeline/sas7bdat/data.rs`
  - [ ] T056 [US1] Implement SAS missing value sentinel detection: check first byte against all 28 patterns, emit `null` instead of float in `src/pipeline/sas7bdat/data.rs`
  - [ ] T057 [US1] Implement date conversion: `sas_days - 3653` → Polars Date (days since Unix epoch) in `src/pipeline/sas7bdat/data.rs`
  - [ ] T058 [P] [US1] Implement datetime conversion: `(sas_seconds - 315_619_200) * 1000` → Polars Datetime (ms since Unix epoch) in `src/pipeline/sas7bdat/data.rs`
  - [ ] T059 [P] [US1] Implement time conversion: `sas_seconds * 1_000_000_000` → Polars Time (ns since midnight) in `src/pipeline/sas7bdat/data.rs`
  - [ ] T060 [US1] Implement character column decoding: read fixed-width bytes, decode via `encoding_rs` (UTF-8, Latin-1, Windows-1252, CJK), trim trailing spaces in `src/pipeline/sas7bdat/data.rs`
  - [ ] T061 [US1] Apply page decompression (RLE or RDC) before row extraction when compression is detected in `src/pipeline/sas7bdat/data.rs`
- **Status:** Pending

---

### TASK-10: Implement Public API and DataFrame Assembly

- **Category:** PERF
- **Priority:** P0
- **Dependencies:** TASK-09
- **Files:** `src/pipeline/sas7bdat/mod.rs`
- **Description:** Implement the two public API functions that serve as the module's external interface: `load_sas7bdat()` for full file loading with DataFrame assembly, and `get_sas7bdat_columns()` for metadata-only column name extraction.
- **Acceptance Criteria:**
  - [ ] T062 [US1] Implement `load_sas7bdat(path) -> Result<(DataFrame, usize, usize, f64)>`: parse header, validate (non-zero rows, supported encoding), extract columns, iterate pages, accumulate per-column vectors, build Polars Series (Float64, Utf8, Date, Datetime, Time), assemble DataFrame, compute memory estimate. Must process page-by-page without buffering entire file (NFR-2). In `src/pipeline/sas7bdat/mod.rs`
  - [ ] T063 [US1] Implement `get_sas7bdat_columns(path) -> Result<Vec<String>>`: parse header + metadata pages only (stop at first data page), return column names in `src/pipeline/sas7bdat/mod.rs`
  - [ ] T064 [US1] Add page-based progress bar (`pages parsed / total pages`) using indicatif, consistent with existing loader progress style in `src/pipeline/sas7bdat/mod.rs`
  - [ ] T065 [US1] Validate `SasError::ZeroRows` when `row_count == 0` in `src/pipeline/sas7bdat/mod.rs`
- **Status:** Pending

---

### TASK-11: Integrate with Loader Module

- **Category:** UX
- **Priority:** P0
- **Dependencies:** TASK-10
- **Files:** `src/pipeline/loader.rs`
- **Description:** Wire the SAS7BDAT parser into the existing dataset loader so that `.sas7bdat` files are recognized and loaded through the standard pipeline entry point.
- **Acceptance Criteria:**
  - [ ] T066 [US1] Add `"sas7bdat"` match arm to `get_column_names()` calling `get_sas7bdat_columns()` in `src/pipeline/loader.rs`
  - [ ] T067 [US1] Add `"sas7bdat"` match arm to `load_dataset_with_progress()` calling `load_sas7bdat()` in `src/pipeline/loader.rs`
  - [ ] T068 [P] [US1] Update unsupported format error message to list `sas7bdat` as a valid format in `src/pipeline/loader.rs`
- **Status:** Pending

---

### TASK-12: Handle Output Format Defaulting

- **Category:** UX
- **Priority:** P1
- **Dependencies:** TASK-11
- **Files:** `src/main.rs`
- **Description:** When the input is a SAS7BDAT file, default the output format to Parquet (since SAS7BDAT write is not supported). The user can override this with `--output`.
- **Acceptance Criteria:**
  - [ ] T069 [US1] In `output_path()` derivation: when input extension is `sas7bdat`, default output extension to `.parquet` instead of preserving input extension in `src/main.rs`
- **Status:** Pending

---

## Phase 4: User Story 2 — TUI File Selector and CLI Integration [FR-1, FR-8, FR-9]

**Story Goal:** As a user, I can see `.sas7bdat` files in the TUI file browser, select them as input, and have the CLI accept them via `--input`.

**Independent Test Criteria:** `.sas7bdat` files appear in the file selector; `--input data.sas7bdat` is accepted by CLI argument parser.

### TASK-13: Integrate with TUI File Selector

- **Category:** UX
- **Priority:** P1
- **Dependencies:** TASK-11
- **Files:** `src/cli/config_menu.rs`
- **Description:** Update the TUI file browser filter to display `.sas7bdat` files alongside `.csv` and `.parquet` files.
- **Acceptance Criteria:**
  - [ ] T070 [US2] Add `e.eq_ignore_ascii_case("sas7bdat")` to `is_valid_data_file()` in `src/cli/config_menu.rs`
- **Status:** Pending

---

### TASK-14: Update CLI Args and Help Text

- **Category:** UX
- **Priority:** P1
- **Dependencies:** TASK-11
- **Files:** `src/cli/args.rs`
- **Description:** Update CLI argument help text to document SAS7BDAT as an accepted input format and the Parquet output default.
- **Acceptance Criteria:**
  - [ ] T071 [US2] Update `--input` help text to: "Input dataset file (CSV, Parquet, or SAS7BDAT)" in `src/cli/args.rs`
  - [ ] T072 [P] [US2] Update `--output` help text to mention SAS7BDAT input defaults to Parquet output in `src/cli/args.rs`
- **Status:** Pending

---

## Phase 5: User Story 3 — SAS7BDAT-to-Parquet/CSV Conversion [FR-7]

**Story Goal:** As a user, I can convert a SAS7BDAT file to Parquet or CSV format via the TUI `[F]` key or CLI convert subcommand.

**Independent Test Criteria:** `lophi convert data.sas7bdat` produces a valid Parquet file; pressing `[F]` in TUI on a SAS7BDAT file converts it successfully.

### TASK-15: Integrate with Convert Subcommand

- **Category:** UX
- **Priority:** P1
- **Dependencies:** TASK-10
- **Files:** `src/cli/convert.rs`
- **Description:** Extend the `run_convert()` function to accept `.sas7bdat` input and convert to Parquet (default) or CSV.
- **Acceptance Criteria:**
  - [ ] T073 [US3] Detect `.sas7bdat` extension on input in `run_convert()` in `src/cli/convert.rs`
  - [ ] T074 [US3] Load SAS7BDAT via `load_sas7bdat()` and write to Parquet using existing `ParquetWriter` in `src/cli/convert.rs`
  - [ ] T075 [P] [US3] Support SAS7BDAT→CSV conversion when `--output` specifies `.csv` extension in `src/cli/convert.rs`
- **Status:** Pending

---

### TASK-16: Update TUI Conversion Flow

- **Category:** UX
- **Priority:** P1
- **Dependencies:** TASK-15
- **Files:** `src/cli/config_menu.rs`
- **Description:** Update the `[F]` key handler to support SAS7BDAT→Parquet conversion with appropriate progress messaging.
- **Acceptance Criteria:**
  - [ ] T076 [US3] Update `[F]` key handler to detect `.sas7bdat` input and route to SAS7BDAT conversion in `src/cli/config_menu.rs`
  - [ ] T077 [P] [US3] Display conversion message "Converting SAS7BDAT to Parquet..." during conversion in `src/cli/config_menu.rs`
- **Status:** Pending

---

## Phase 6: Testing

Goal: Comprehensive test coverage for the SAS7BDAT parser including unit tests, integration tests, and test fixtures. Per Constitution Principle 5, any bugs discovered during implementation MUST include a regression test that fails without the fix and passes with it.

### TASK-17: Create Test Reference Files

- **Category:** TEST
- **Priority:** P1
- **Dependencies:** TASK-10
- **Files:** `tests/fixtures/` (new directory)
- **Description:** Generate reference SAS7BDAT files using Python/pandas for use in unit and integration tests. Also generate expected-output CSV/Parquet for value comparison.
- **Acceptance Criteria:**
  - [ ] T078 Create Python script `tests/fixtures/generate_sas7bdat_fixtures.py` to generate all reference files
  - [ ] T079 Generate `basic_64bit_le.sas7bdat` — standard 64-bit little-endian, mixed numeric/character types
  - [ ] T080 [P] Generate `basic_32bit.sas7bdat` — 32-bit format variant
  - [ ] T081 [P] Generate `compressed_rle.sas7bdat` — RLE compressed (COMPRESS=CHAR)
  - [ ] T082 [P] Generate `compressed_rdc.sas7bdat` — RDC compressed (COMPRESS=BINARY). Note: pandas may not support COMPRESS=BINARY natively; source from Parso's Java test suite or hand-craft binary test data if needed
  - [ ] T083 [P] Generate `with_dates.sas7bdat` — DATE, DATETIME, TIME columns
  - [ ] T084 [P] Generate `with_missing.sas7bdat` — standard and special missing values (`.`, `.A`-`.Z`, `._`)
  - [ ] T085 [P] Generate `latin1_encoding.sas7bdat` — Latin-1 encoded strings
  - [ ] T086 [P] Generate `truncated_numerics.sas7bdat` — columns with LENGTH < 8
  - [ ] T087 [P] Generate `zero_rows.sas7bdat` — empty dataset (header only)
  - [ ] T088 [P] Generate `truncated_file.sas7bdat` — file cut short mid-page
  - [ ] T089 Generate expected-output CSV and Parquet files for each fixture for value comparison (Parquet preserves IEEE 754 doubles for bit-exact numeric fidelity verification per NFR-3)
- **Status:** Pending

---

### TASK-18: Add Test Fixture Helpers

- **Category:** TEST
- **Priority:** P1
- **Dependencies:** TASK-17
- **Files:** `tests/common/mod.rs`
- **Description:** Add helper functions for SAS7BDAT test fixtures to the shared test module.
- **Acceptance Criteria:**
  - [ ] T090 Add `sas7bdat_fixture_path(name: &str) -> PathBuf` helper to locate test fixture files in `tests/common/mod.rs`
  - [ ] T091 [P] Add `create_temp_sas7bdat(name: &str) -> TempDir` helper that copies fixture file to temp directory in `tests/common/mod.rs`
- **Status:** Pending

---

### TASK-19: Unit Tests for Parser Modules

- **Category:** TEST
- **Priority:** P1
- **Dependencies:** TASK-10, TASK-17
- **Files:** `src/pipeline/sas7bdat/*.rs` (inline `#[cfg(test)]` modules)
- **Description:** Add unit tests to each parser module covering the critical parsing logic.
- **Acceptance Criteria:**
  - [ ] T092 Add unit tests for `constants.rs`: magic number length assertion, encoding table completeness in `src/pipeline/sas7bdat/constants.rs`
  - [ ] T093 Add unit tests for `header.rs`: 64-bit LE header parse, 32-bit header parse, big-endian detection, invalid magic, truncated header, SAS release version extraction, encoding ID 0 (Unspecified) fallback in `src/pipeline/sas7bdat/header.rs`
  - [ ] T094 [P] Add unit tests for `decompress.rs`: RLE round-trip on known data, all 16 RLE commands individually, RDC round-trip, empty input, buffer overflow protection in `src/pipeline/sas7bdat/decompress.rs`
  - [ ] T095 [P] Add unit tests for `data.rs`: truncated numeric reconstruction (3,4,5,6,7,8 bytes) with bit-exact comparison via `f64::to_le_bytes()`, missing value sentinel detection (`.`, `.A`-`.Z`, `._`), date/datetime epoch conversion, character encoding (Latin-1, UTF-8) in `src/pipeline/sas7bdat/data.rs`
  - [ ] T096 [P] Add unit tests for `column.rs`: format string parsing (DATE9. → Date, DATETIME20. → Datetime, BEST12. → Float64) in `src/pipeline/sas7bdat/column.rs`
- **Status:** Pending

---

### TASK-20: Integration Tests

- **Category:** TEST
- **Priority:** P1
- **Dependencies:** TASK-11, TASK-17, TASK-18
- **Files:** `tests/test_sas7bdat.rs` (new)
- **Description:** End-to-end integration tests using reference fixture files, verifying data shapes, column types, numeric values, date values, string values, null positions, and full pipeline execution.
- **Acceptance Criteria:**
  - [ ] T097 Test `test_load_sas7bdat_basic`: load and verify shape, column names, sample values (also implicitly exercises page-based progress bar path from T064) in `tests/test_sas7bdat.rs`
  - [ ] T098 [P] Test `test_load_sas7bdat_compressed_rle`: load RLE-compressed file, verify correctness in `tests/test_sas7bdat.rs`
  - [ ] T099 [P] Test `test_load_sas7bdat_compressed_rdc`: load RDC-compressed file, verify correctness in `tests/test_sas7bdat.rs`
  - [ ] T100 [P] Test `test_load_sas7bdat_dates`: verify date/datetime conversion against expected values in `tests/test_sas7bdat.rs`
  - [ ] T101 [P] Test `test_load_sas7bdat_missing_values`: verify nulls in expected positions in `tests/test_sas7bdat.rs`
  - [ ] T102 [P] Test `test_load_sas7bdat_encoding_latin1`: verify Latin-1 string decoding in `tests/test_sas7bdat.rs`
  - [ ] T103 [P] Test `test_get_column_names_sas7bdat`: metadata-only extraction in `tests/test_sas7bdat.rs`
  - [ ] T104 Test `test_sas7bdat_zero_rows_rejected`: clear error for empty files in `tests/test_sas7bdat.rs`
  - [ ] T105 [P] Test `test_sas7bdat_truncated_file`: clear error for corrupted files in `tests/test_sas7bdat.rs`
  - [ ] T106 [P] Test `test_sas7bdat_invalid_magic`: clear error for non-SAS files in `tests/test_sas7bdat.rs`
  - [ ] T107 Test `test_full_pipeline_sas7bdat`: end-to-end load → missing → IV → correlation → save in `tests/test_sas7bdat.rs`
  - [ ] T107b [P] Test `test_sas7bdat_output_csv_override`: verify `--output data.csv` produces CSV when input is SAS7BDAT (FR-9 override path) in `tests/test_sas7bdat.rs`
  - [ ] T108 [P] Test `test_convert_sas7bdat_to_parquet`: conversion produces valid Parquet in `tests/test_sas7bdat.rs`
  - [ ] T109 [P] Test `test_convert_sas7bdat_to_csv`: conversion produces valid CSV in `tests/test_sas7bdat.rs`
- **Status:** Pending

---

## Phase 6b: Benchmarks

Goal: Criterion benchmark for SAS7BDAT parser performance (Constitution Principle 2 MUST).

### TASK-20b: SAS7BDAT Parsing Benchmark

- **Category:** PERF
- **Priority:** P1
- **Dependencies:** TASK-10, TASK-17
- **Files:** `benches/sas7bdat_benchmark.rs` (new)
- **Description:** Add Criterion benchmark measuring page parsing throughput for uncompressed, RLE, and RDC SAS7BDAT files. Required by Constitution Principle 2: "Performance regressions MUST be detected via Criterion benchmarks."
- **Acceptance Criteria:**
  - [ ] T118 Create `benches/sas7bdat_benchmark.rs` with Criterion benchmark group for `load_sas7bdat()` on `basic_64bit_le.sas7bdat`
  - [ ] T119 [P] Add benchmark case for RLE-compressed fixture (`compressed_rle.sas7bdat`)
  - [ ] T120 [P] Add benchmark case for RDC-compressed fixture (`compressed_rdc.sas7bdat`)
- **Status:** Pending

---

## Phase 7: Polish and Cross-Cutting Concerns

Goal: Documentation, ADR, and final cleanup.

### TASK-21: Update CLAUDE.md

- **Category:** TRANS
- **Priority:** P2
- **Dependencies:** TASK-11, TASK-13, TASK-14, TASK-15, TASK-16
- **Files:** `CLAUDE.md`
- **Description:** Update project CLAUDE.md with SAS7BDAT module structure, loader changes, TUI changes, encoding_rs dependency, and output defaults.
- **Acceptance Criteria:**
  - [ ] T110 Add `sas7bdat/` submodule to Module Structure section in `CLAUDE.md`
  - [ ] T111 [P] Update loader description to include SAS7BDAT in `CLAUDE.md`
  - [ ] T112 [P] Update TUI file selector documentation in `CLAUDE.md`
  - [ ] T113 [P] Add SAS7BDAT output format defaulting note in `CLAUDE.md`
  - [ ] T114 [P] Add `encoding_rs` to Key Dependencies section in `CLAUDE.md`
- **Status:** Pending

---

### TASK-22: Update Project Documentation

- **Category:** TRANS
- **Priority:** P2
- **Dependencies:** TASK-11, TASK-13, TASK-15
- **Files:** `docs/user-guide.md`, `docs/architecture.md`
- **Description:** Update external documentation to reflect SAS7BDAT support.
- **Acceptance Criteria:**
  - [ ] T115 Update `docs/user-guide.md`: SAS7BDAT as accepted input, conversion options, output defaults
  - [ ] T116 [P] Update `docs/architecture.md`: new `sas7bdat/` module in module structure diagram
- **Status:** Pending

---

### TASK-23: Create ADR-009

- **Category:** TRANS
- **Priority:** P2
- **Dependencies:** TASK-11
- **Files:** `docs/adr/009-sas7bdat-pure-rust-parser.md` (new)
- **Description:** Document the architectural decision to build a custom pure Rust SAS7BDAT parser, including context, decision rationale, alternatives considered (readstat-rs, quick-sas7bdat, Python shelling), and consequences.
- **Acceptance Criteria:**
  - [ ] T117 Create ADR-009 with context, decision, alternatives, and consequences in `docs/adr/009-sas7bdat-pure-rust-parser.md`
- **Status:** Pending

---

## Dependency Graph

```
Phase 1: Setup
  T001 (Cargo.toml) ─┐
  T002 (mkdir)  ──────┤
  T003 (mod.rs) ──────┘
          │
          v
Phase 2: Foundational
  TASK-01 (Constants) ─────┬─────────────────────────────────┐
  TASK-02 (Errors)  ───────┤                                 │
  TASK-03 (Types)   ───────┘                                 │
          │                                                  │
          v                                                  │
Phase 3: US1 — Core Parser + Loading                         │
  TASK-04 (Header) ──→ TASK-05 (Pages) ──→ TASK-06 (Subhdr) │
                                               │             │
                                               v             │
                                           TASK-07 (Column)  │
                                               │             │
  TASK-08 (Decompress) ←──────────────────────────────────────┘
          │                    │
          v                    v
      TASK-09 (Data Extraction)
          │
          v
      TASK-10 (Public API + DataFrame)
          │
          v
      TASK-11 (Loader Integration)
          │
          v
      TASK-12 (Output Defaults)
          │
          ├──────────────────────────────────────┐
          v                                      v
Phase 4: US2 — TUI/CLI                   Phase 5: US3 — Convert
  TASK-13 (File Selector)                   TASK-15 (Convert Subcmd)
  TASK-14 (CLI Args)                        TASK-16 (TUI Convert)
          │                                      │
          └──────────┬───────────────────────────┘
                     v
Phase 6: Testing
  TASK-17 (Fixtures) ──→ TASK-18 (Helpers) ──→ TASK-19 (Unit Tests)
                                             ──→ TASK-20 (Integration Tests)
                     │
                     v
Phase 7: Polish
  TASK-21 (CLAUDE.md)
  TASK-22 (Docs)
  TASK-23 (ADR-009)
```

---

## Parallel Execution Opportunities

### Within Phase 2 (Foundational):
- TASK-01, TASK-02, TASK-03 can be worked on in parallel (they share no file dependencies beyond the module skeleton from Phase 1)

### Within Phase 3 (US1):
- TASK-08 (Decompress) can be built in parallel with TASK-04→05→06→07 chain (only depends on TASK-01 constants)
- T028/T029/T031/T033 within TASK-04 are parallelizable (independent header fields)

### Phase 4 and Phase 5 are independent:
- US2 (TUI/CLI integration) and US3 (Convert) can be done in parallel after TASK-11

### Within Phase 6 (Testing):
- After fixtures (TASK-17), unit tests (TASK-19) and integration tests (TASK-20) can be developed in parallel
- Most individual test cases within TASK-19 and TASK-20 are marked [P] for parallel execution

### Within Phase 7 (Polish):
- TASK-21, TASK-22, TASK-23 are all independent and parallelizable

---

## Implementation Strategy

### MVP Scope (Recommended)
**Phase 1 + Phase 2 + Phase 3 (US1)**: Setup → Constants/Types/Errors → Full parser + Loader integration. This delivers the core value: users can load SAS7BDAT files into the pipeline. Tasks T001–T069.

### Increment 2
**Phase 4 + Phase 5 (US2 + US3)**: TUI/CLI polish and conversion support. Tasks T070–T077.

### Increment 3
**Phase 6 (Testing)**: Comprehensive test suite. Tasks T078–T109.

### Increment 4
**Phase 7 (Polish)**: Documentation and ADR. Tasks T110–T117.

---

## Summary

| Metric | Value |
|--------|-------|
| **Total tasks** | 121 (T001–T120 + T107b) |
| **Setup tasks** | 3 (T001–T003) |
| **Foundational tasks** | 22 (T004–T025) |
| **US1 tasks (Core Parser + Loading)** | 44 (T026–T069) |
| **US2 tasks (TUI/CLI)** | 3 (T070–T072) |
| **US3 tasks (Convert)** | 5 (T073–T077) |
| **Testing tasks** | 33 (T078–T109 + T107b) |
| **Benchmark tasks** | 3 (T118–T120) |
| **Polish tasks** | 8 (T110–T117) |
| **Parallelizable tasks** | 63 (marked [P]) |
| **Task format validation** | All 121 tasks follow checklist format (checkbox, ID, labels, file paths) |

---

## Completion Checklist

- [ ] All tasks marked Done
- [ ] CI passes (clippy + fmt + tests)
- [ ] Benchmarks run (if PERF tasks present)
- [ ] CLAUDE.md updated (if architecture changed)
- [ ] Constitution compliance verified
