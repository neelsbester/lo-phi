# Feature Specification: SAS7BDAT File Format Support

**Version:** 1.0.0
**Date:** 2026-02-01
**Status:** Draft

---

## Summary

Add read-only SAS7BDAT (.sas7bdat) file format support to Lo-phi so that users working with SAS-exported datasets can load them directly into the feature reduction pipeline without manual pre-conversion. This includes a pure Rust binary parser that reads SAS7BDAT files and produces Polars DataFrames, integration with the existing loader, CLI, and TUI, and a SAS7BDAT-to-Parquet/CSV conversion utility. Output remains in open formats (CSV or Parquet); no SAS7BDAT write support is provided.

## Constitution Alignment

| Principle                       | Applicable | Notes                                                                                           |
| ------------------------------- | ---------- | ----------------------------------------------------------------------------------------------- |
| 1. Statistical Correctness      | Yes        | Parsed numeric values must be bit-accurate; no silent precision loss or value corruption         |
| 2. Performance at Scale         | Yes        | SAS7BDAT files can be multi-GB; parsing must stream page-by-page, not buffer entire file        |
| 3. Transparent Decision-Making  | Yes        | Parse errors/warnings (unsupported compression, encoding) must be reported, never silenced       |
| 4. Ergonomic TUI/CLI            | Yes        | SAS7BDAT files appear in file selector; conversion follows the existing [F] key pattern          |
| 5. Rigorous Testing             | Yes        | Parser needs known-answer tests against reference files covering 32/64-bit, compressed, encoded  |

## Requirements

### Functional Requirements

1. **[FR-1] SAS7BDAT File Loading**: The system must accept `.sas7bdat` files as input wherever CSV and Parquet files are currently accepted (CLI `--input` flag, TUI file selector).

2. **[FR-2] Binary Header Parsing**: The parser must correctly read the SAS7BDAT file header, extracting: magic number validation, 32-bit vs 64-bit alignment, byte order (endianness), page size, page count, row count, row length, column count, dataset name, encoding, and creation/modification timestamps.

3. **[FR-3] Page and Subheader Processing**: The parser must process all standard page types (meta, data, mixed, AMD) and subheader types (row size, column size, column text, column name, column attributes, column format/label, subheader counts).

4. **[FR-4] Data Type Support**: The parser must correctly extract:
   - Numeric columns (IEEE 754 floating-point, including truncated representations of 3-8 bytes)
   - Character/string columns (fixed-width, with encoding-aware decoding)
   - SAS date/datetime values (converted from SAS epoch 1960-01-01 to standard representations) for the core format set: `DATE`, `DATETIME`, `TIME`, `DDMMYY`, `MMDDYY`, `YYMMDD` and their width variants; columns with other SAS formats are treated as raw numeric
   - SAS missing value sentinels (mapped to null/NaN in the resulting DataFrame)

5. **[FR-5] Compression Support**: The parser must decompress data compressed with:
   - RLE (Run-Length Encoding) - the `COMPRESS=CHAR` / `COMPRESS=YES` format
   - RDC (Ross Data Compression) - the `COMPRESS=BINARY` format

6. **[FR-6] Polars DataFrame Output**: Parsed data must be returned as a Polars `DataFrame` with appropriate column types (Float64 for numeric, String for character, Date/Datetime for SAS date columns) so it integrates seamlessly with the existing pipeline stages (missing analysis, IV analysis, correlation analysis).

7. **[FR-7] SAS7BDAT-to-Parquet/CSV Conversion**: Users must be able to convert a SAS7BDAT file to Parquet or CSV format, accessible via:
   - The TUI conversion option (extending the existing `[F]` key feature)
   - The CLI `convert` subcommand (extending to accept SAS7BDAT input)

8. **[FR-8] TUI File Selector Integration**: The TUI file browser must display `.sas7bdat` files alongside `.csv` and `.parquet` files when browsing for input data.

9. **[FR-9] Output Format Preservation**: When a SAS7BDAT file is loaded as pipeline input, the reduced output dataset must default to Parquet format (since SAS7BDAT write is not supported). Users may override this with `--output` to choose CSV.

10. **[FR-10] Schema Inference**: Column names, types, and the full column list must be extractable from SAS7BDAT metadata without reading all data rows, matching the behavior of `get_column_names()` for CSV and Parquet.

### Non-Functional Requirements

1. **[NFR-1] Pure Rust Implementation**: The SAS7BDAT parser must be implemented in pure Rust with no C/C++ FFI dependencies, keeping the build toolchain simple and cross-compilation straightforward.

2. **[NFR-2] Streaming/Page-Level Processing**: The parser must read data page-by-page rather than loading the entire file into memory, supporting files larger than available RAM.

3. **[NFR-3] Numeric Fidelity**: IEEE 754 double-precision values extracted from SAS7BDAT files must be bit-identical to the values SAS wrote. No rounding or precision loss is acceptable for numeric data.

4. **[NFR-4] Error Reporting**: Parse failures (corrupted file, unsupported features, unknown compression) must produce clear error messages identifying the file, the byte offset (when available), and the nature of the failure.

5. **[NFR-5] Character Encoding**: The parser must support at least UTF-8 and Latin-1 (ISO-8859-1) encoded string data, with the encoding determined from the file header metadata. Unsupported encodings must produce a clear error rather than garbled data.

## Scope

### In Scope

- Pure Rust SAS7BDAT binary parser (header, pages, subheaders, data rows)
- RLE and RDC decompression
- 32-bit and 64-bit SAS7BDAT file variants
- Big-endian and little-endian byte order support
- Numeric (float) and character (string) column extraction
- SAS date/datetime to Polars date/datetime conversion
- SAS missing value handling (mapped to null)
- Integration with existing loader, CLI args, TUI file selector, and convert subcommand
- Default output as Parquet when input is SAS7BDAT
- Unit and integration tests with reference SAS7BDAT files

### Out of Scope

- SAS7BDAT write/export support
- SAS transport format (.xpt / XPORT) support
- SAS catalog files (.sas7bcat) for value labels and formats
- Encrypted or password-protected SAS7BDAT files
- SAS-specific custom informats/formats beyond date/datetime
- Streaming/lazy evaluation of SAS7BDAT (Polars LazyFrame) - the parser will produce an eager DataFrame

## User Scenarios & Testing

### Scenario 1: Direct Pipeline Execution on SAS7BDAT File

**Given** a user has a `.sas7bdat` file containing their dataset
**When** they run `lophi data.sas7bdat` or select it via the TUI file browser
**Then** the file is parsed and loaded into the pipeline, the reduction analysis runs identically to CSV/Parquet input, and the reduced output is saved as `data_reduced.parquet`

### Scenario 2: SAS7BDAT to Parquet Conversion

**Given** a user wants to convert a SAS7BDAT file to Parquet for repeated use
**When** they press `[F]` in the TUI (or run `lophi convert data.sas7bdat`)
**Then** the file is converted to `data.parquet` in the same directory, and the pipeline continues using the Parquet file

### Scenario 3: Large Compressed SAS7BDAT File

**Given** a user has a multi-GB SAS7BDAT file compressed with RLE
**When** they load it into the pipeline
**Then** the file is decompressed and parsed page-by-page without excessive memory usage, and a page-based progress indicator (pages parsed / total pages from header) is displayed during loading

### Scenario 4: SAS7BDAT with Date Columns

**Given** a SAS7BDAT file contains date and datetime columns
**When** it is parsed
**Then** date columns appear as proper date types in the DataFrame, not as raw numeric SAS day offsets

### Scenario 5: Corrupted or Unsupported SAS7BDAT File

**Given** a user provides a corrupted, truncated, or zero-row SAS7BDAT file, or one using unsupported features (encryption, exotic encoding)
**When** they attempt to load it
**Then** the system displays a clear error message identifying the problem (e.g., "file contains no data rows", "file is truncated", "unsupported encryption") and does not crash or produce partial/incorrect results

## Technical Constraints

- Must use only pure Rust crates (no C FFI bindings)
- Must produce a Polars `DataFrame` as output (consistent with existing loader interface)
- The SAS7BDAT format is reverse-engineered, not officially documented; the [Shotwell specification](https://github.com/BioStatMatt/sas7bdat) and the [pandas](https://github.com/pandas-dev/pandas/blob/main/pandas/io/sas/sas7bdat.py) / [Parso](https://github.com/epam/parso) (Apache 2.0) implementations serve as authoritative references
- RDC (Ross Data Compression) decompression is less commonly encountered and less well-documented than RLE; implementation may require careful study of the Parso Java reference

## Assumptions

- The vast majority of real-world SAS7BDAT files use little-endian byte order (x86/x64 origin); big-endian support is included for correctness but is a lower-priority test path
- Most SAS7BDAT files encountered will use UTF-8 or Latin-1 encoding; other encodings are supported on a best-effort basis
- Users do not need SAS value labels/formats (from `.sas7bcat` files); raw values are sufficient
- Date/datetime columns will be detected via SAS format metadata in the column format subheader, recognizing the core set: `DATE`, `DATETIME`, `TIME`, `DDMMYY`, `MMDDYY`, `YYMMDD` (and width variants like `DATE9.`, `DATETIME20.`); all other SAS formats are treated as raw numeric
- The pipeline's existing analysis stages (missing, IV, correlation) require no changes; they operate on the Polars DataFrame regardless of the source format

## Success Criteria

- Users can load any standard (unencrypted, unpassworded) SAS7BDAT file directly into the Lo-phi pipeline without pre-conversion
- Numeric values extracted from SAS7BDAT files are bit-identical to the values produced by SAS, pandas, or Parso for the same file (per NFR-3)
- SAS7BDAT files up to 5 GB can be processed without requiring more than 2x the peak RSS of loading the equivalent Parquet file (same data, default Parquet compression)
- All existing tests continue to pass (no regressions)
- The feature ships with at least 10 targeted unit tests covering header parsing, decompression, data type extraction, date conversion, missing values, and error handling (minimum threshold; the full test plan targets 32+ tests)
- SAS7BDAT-to-Parquet conversion produces files that are byte-for-byte identical in data content to conversion via pandas or Parso (column names, types, and values match)

## Clarifications

### Session 2026-02-01

- Q: How should loading progress be tracked for SAS7BDAT files? → A: Page-based progress (pages parsed / total pages from header)
- Q: How should zero-row SAS7BDAT files be handled? → A: Reject with a clear error ("file contains no data rows")
- Q: How broad should SAS date format recognition be? → A: Core set: DATE, DATETIME, TIME, DDMMYY, MMDDYY, YYMMDD and their width variants; all other formats treated as raw numeric

## Acceptance Criteria

- [ ] `.sas7bdat` files are accepted by `--input` CLI flag
- [ ] `.sas7bdat` files appear in TUI file browser
- [ ] TUI `[F]` key converts SAS7BDAT to Parquet
- [ ] CLI `convert` subcommand accepts SAS7BDAT input
- [ ] 32-bit and 64-bit SAS7BDAT files parse correctly
- [ ] Little-endian and big-endian files parse correctly
- [ ] RLE compressed files decompress and parse correctly
- [ ] RDC compressed files decompress and parse correctly
- [ ] Numeric columns produce correct Float64 values
- [ ] Character columns decode correctly with UTF-8 and Latin-1
- [ ] SAS date/datetime columns convert to Polars date types
- [ ] SAS missing values map to null in the DataFrame
- [ ] Reduced output defaults to Parquet when input is SAS7BDAT
- [ ] Zero-row SAS7BDAT files are rejected with a clear error message
- [ ] Truncated SAS7BDAT files are rejected with a clear error message
- [ ] Corrupted files produce clear error messages without panicking
- [ ] Full pipeline (missing -> IV -> correlation -> save) works end-to-end on SAS7BDAT input
- [ ] No regressions in existing CSV/Parquet test suite
