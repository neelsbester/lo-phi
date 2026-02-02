# Data Model: SAS7BDAT File Format Support

**Date:** 2026-02-01

---

## Core Types

### SasHeader

Parsed from the file header (first `header_length` bytes).

```rust
pub struct SasHeader {
    /// true = 64-bit, false = 32-bit
    pub is_64bit: bool,
    /// true = little-endian, false = big-endian
    pub is_little_endian: bool,
    /// Character encoding identifier (maps to encoding_rs encoding)
    pub encoding: SasEncoding,
    /// Page size in bytes
    pub page_size: u32,
    /// Total number of pages in the file
    pub page_count: u64,
    /// Total number of data rows
    pub row_count: u64,
    /// Length of a single row in bytes
    pub row_length: u64,
    /// Number of columns
    pub column_count: u64,
    /// Dataset name (from header metadata)
    pub dataset_name: String,
    /// File creation timestamp (seconds since SAS epoch 1960-01-01)
    pub created: f64,
    /// File modification timestamp (seconds since SAS epoch 1960-01-01)
    pub modified: f64,
    /// Total header length in bytes (offset to first page)
    pub header_length: u64,
    /// Compression type detected from column text subheader
    pub compression: Compression,
    /// OS type (Unix/Windows)
    pub os_type: OsType,
    /// SAS release version string
    pub sas_release: String,
    /// Maximum rows on a mix page
    pub max_rows_on_mix_page: u64,
}
```

### SasColumn

Metadata for a single column, assembled from multiple subheaders.

```rust
pub struct SasColumn {
    /// Column name (decoded from column text + column name subheaders)
    pub name: String,
    /// Column data type
    pub data_type: SasDataType,
    /// Byte offset within a row where this column's data starts
    pub offset: u64,
    /// Byte length of this column's data within a row
    pub length: u32,
    /// SAS format name (e.g., "DATE9.", "BEST12.", "DATETIME20.")
    pub format: String,
    /// Column label (descriptive text)
    pub label: String,
    /// Polars output type (derived from data_type + format)
    pub polars_type: PolarsOutputType,
}
```

### Enums

```rust
pub enum SasDataType {
    Numeric,    // IEEE 754 double (possibly truncated to 3-8 bytes)
    Character,  // Fixed-width encoded string
}

pub enum PolarsOutputType {
    Float64,             // Default for numeric columns
    Date,                // DATE, DDMMYY, MMDDYY, YYMMDD formats
    Datetime,            // DATETIME format
    Time,                // TIME format
    Utf8,                // Character columns
}

pub enum Compression {
    None,
    Rle,    // SASYZCRL — COMPRESS=CHAR/YES
    Rdc,    // SASYZCR2 — COMPRESS=BINARY
}

pub enum OsType {
    Unix,
    Windows,
    Unknown,
}

pub enum SasEncoding {
    Utf8,           // ID 20
    Ascii,          // ID 28
    Latin1,         // ID 29
    Windows1252,    // ID 62
    /// Other encodings supported via encoding_rs
    Other { id: u16, name: &'static str },
    /// Unspecified (ID 0) — fall back to Latin-1
    Unspecified,
}
```

### SasError

```rust
pub enum SasError {
    /// File does not start with the SAS7BDAT magic number
    InvalidMagic,
    /// File is truncated (fewer bytes than header declares)
    TruncatedFile { expected: u64, actual: u64 },
    /// File contains zero data rows
    ZeroRows,
    /// Character encoding is not supported
    UnsupportedEncoding { id: u16 },
    /// Page type is not recognized
    InvalidPageType { page_index: u64, page_type: u16 },
    /// Subheader signature is not recognized
    UnknownSubheader { signature: Vec<u8>, offset: u64 },
    /// Decompression failed
    DecompressionError { page_index: u64, message: String },
    /// Numeric value reconstruction failed
    NumericError { column: String, row: u64, message: String },
    /// I/O error
    Io(std::io::Error),
}
```

---

## Relationships

```
SasHeader 1──────* SasColumn
    │                  │
    │ compression      │ data_type + format
    │ encoding         │      │
    │ is_64bit         │      v
    │ page_size        │  PolarsOutputType
    v                  │
 Page Iterator         v
    │              Row Extraction
    │                  │
    v                  v
 Subheader         DataFrame
 Processing        Assembly
```

- One `SasHeader` describes the file structure
- Multiple `SasColumn` entries (one per column) are assembled from subheader data
- `PolarsOutputType` is derived from `SasDataType` + format string
- Page iteration uses `SasHeader` for page size and count
- Row extraction uses `SasColumn` for offsets, lengths, and type conversion

---

## Validation Rules

| Field | Rule |
|-------|------|
| Magic number | Must match exact 32-byte sequence |
| Page size | Must be > 0 and match file size arithmetic |
| Row count | Must be > 0 (zero-row files rejected with `SasError::ZeroRows`) |
| Row length | Must be > 0 |
| Column count | Must be > 0 and match assembled column metadata count |
| Encoding ID | Must map to a supported encoding or produce `SasError::UnsupportedEncoding` |
| Column offset + length | Must not exceed row_length |
| Page count | header_length + page_count * page_size must not exceed file size |

---

## SAS Date/Datetime Conversion Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `SAS_EPOCH_OFFSET_DAYS` | 3,653 | Days between 1960-01-01 and 1970-01-01 |
| `SAS_EPOCH_OFFSET_SECONDS` | 315,619,200 | Seconds between SAS and Unix epochs |
| `MS_PER_SECOND` | 1,000 | For datetime conversion to milliseconds |
| `NS_PER_SECOND` | 1,000,000,000 | For time conversion to nanoseconds |
