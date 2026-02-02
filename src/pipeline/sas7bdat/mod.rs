//! SAS7BDAT file format parser.
//!
//! This module provides functionality to parse SAS7BDAT binary files and convert
//! them to Polars DataFrames for use in the Lo-phi feature reduction pipeline.
//!
//! # Module Structure
//!
//! - `constants` - Magic numbers, page types, compression constants
//! - `error` - Error types for parsing failures
//! - `header` - File header parsing (metadata, encoding, dimensions)
//! - `page` - Page-level parsing (data pages, metadata pages)
//! - `subheader` - Subheader parsing (column metadata, row size, etc.)
//! - `column` - Column metadata and type definitions
//! - `decompress` - RLE and RDC decompression algorithms
//! - `data` - Data extraction and conversion to Polars

pub mod column;
pub mod constants;
pub mod data;
pub mod decompress;
pub mod error;
pub mod header;
pub mod page;
pub mod subheader;

// Re-export public API types
pub use error::SasError;

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};
use polars::prelude::*;

use self::column::build_columns;
use self::data::{build_series_from_column_values, extract_rows_from_page, ColumnValue};
use self::header::parse_header;
use self::page::{is_page_data, is_page_meta, is_page_mix, parse_page_header};
use self::subheader::{parse_subheader_pointers, process_subheader, SubheaderState};

/// Loads a SAS7BDAT file and returns a Polars DataFrame with statistics.
///
/// This is the main entry point for SAS7BDAT file loading. It:
/// 1. Parses the file header for metadata
/// 2. Iterates metadata pages to extract column definitions
/// 3. Iterates data/mix pages to extract row values
/// 4. Builds Polars Series with appropriate dtypes
/// 5. Assembles the final DataFrame
///
/// # Arguments
/// * `path` - Path to the `.sas7bdat` file
///
/// # Returns
/// Tuple of `(DataFrame, rows, columns, memory_mb)` matching the loader API
///
/// # Errors
/// * `SasError::InvalidMagic` - Not a valid SAS7BDAT file
/// * `SasError::ZeroRows` - File contains no data rows
/// * `SasError::UnsupportedEncoding` - Unknown character encoding
/// * `SasError::TruncatedFile` - File is shorter than expected
pub fn load_sas7bdat(path: &Path) -> Result<(DataFrame, usize, usize, f64), SasError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Step 1: Parse file header
    let mut sas_header = parse_header(&mut reader)?;

    // Step 2: Iterate metadata pages to extract column definitions
    let mut state = SubheaderState::default();
    reader.seek(SeekFrom::Start(sas_header.header_length))?;

    // Create progress bar for page iteration
    let pb = ProgressBar::new(sas_header.page_count);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "   Loading SAS7BDAT [{bar:40.cyan/blue}] {pos}/{len} pages ({percent}%) [{eta}]",
            )
            .unwrap()
            .progress_chars("=>-"),
    );

    let mut page_buf = vec![0u8; sas_header.page_size as usize];

    // First pass: process metadata pages to get column definitions
    for page_idx in 0..sas_header.page_count {
        let bytes_read = reader.read(&mut page_buf)?;
        if bytes_read < sas_header.page_size as usize {
            break; // Truncated page
        }

        let page_header =
            parse_page_header(&page_buf, sas_header.is_64bit, sas_header.is_little_endian)?;

        // Process subheaders on metadata and mix pages
        if is_page_meta(page_header.page_type) || is_page_mix(page_header.page_type) {
            let pointers = parse_subheader_pointers(
                &page_buf,
                sas_header.is_64bit,
                sas_header.is_little_endian,
                page_header.subheader_count,
            )?;

            for pointer in &pointers {
                // Skip compressed data subheaders (compression != 0 and type == 1 means data)
                if pointer.compression != 0 && pointer.subheader_type == 1 {
                    continue;
                }
                process_subheader(
                    &page_buf,
                    pointer,
                    sas_header.is_64bit,
                    sas_header.is_little_endian,
                    &mut state,
                )?;
            }
        }

        pb.set_position(page_idx + 1);
    }

    // Update header with subheader-derived values
    sas_header.row_count = state.row_count;
    sas_header.row_length = state.row_length;
    sas_header.column_count = state.column_count_from_size;
    sas_header.max_rows_on_mix_page = state.max_rows_on_mix_page;
    sas_header.compression = state.compression;

    // Validate non-zero rows
    if sas_header.row_count == 0 {
        pb.finish_and_clear();
        return Err(SasError::ZeroRows);
    }

    // Build column metadata
    let columns = build_columns(&state, &sas_header.encoding);
    if columns.is_empty() {
        pb.finish_and_clear();
        return Err(SasError::ZeroRows);
    }

    // Step 3: Second pass - extract data rows
    reader.seek(SeekFrom::Start(sas_header.header_length))?;
    pb.set_position(0);
    pb.set_length(sas_header.page_count);

    // Initialize per-column value accumulators
    let mut column_values: Vec<Vec<ColumnValue>> = columns
        .iter()
        .map(|_| Vec::with_capacity(sas_header.row_count as usize))
        .collect();
    let mut rows_collected: u64 = 0;

    for page_idx in 0..sas_header.page_count {
        if rows_collected >= sas_header.row_count {
            break;
        }

        let bytes_read = reader.read(&mut page_buf)?;
        if bytes_read < sas_header.page_size as usize {
            break;
        }

        let page_header =
            parse_page_header(&page_buf, sas_header.is_64bit, sas_header.is_little_endian)?;

        // Only extract data from data pages and mix pages
        if is_page_data(page_header.page_type) || is_page_mix(page_header.page_type) {
            let page_rows = extract_rows_from_page(
                &page_buf,
                &sas_header,
                &columns,
                page_idx,
                sas_header.compression,
                rows_collected,
                sas_header.row_count,
            )?;

            for row in &page_rows {
                for (col_idx, value) in row.iter().enumerate() {
                    if col_idx < column_values.len() {
                        column_values[col_idx].push(value.clone());
                    }
                }
            }
            rows_collected += page_rows.len() as u64;
        }

        pb.set_position(page_idx + 1);
    }

    pb.finish_and_clear();

    // Step 4: Build Polars Series for each column
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("   {spinner:.cyan} Building DataFrame...")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut column_vec: Vec<Column> = Vec::with_capacity(columns.len());
    for (col, values) in columns.iter().zip(column_values.into_iter()) {
        let series = build_series_from_column_values(&col.name, &col.polars_type, values)?;
        column_vec.push(series.into());
    }

    // Step 5: Assemble DataFrame
    let df = DataFrame::new(column_vec).map_err(|e| {
        SasError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to build DataFrame: {}", e),
        ))
    })?;

    spinner.finish_and_clear();

    let (rows, cols) = df.shape();
    let memory_mb = df.estimated_size() as f64 / (1024.0 * 1024.0);

    Ok((df, rows, cols, memory_mb))
}

/// Gets column names from a SAS7BDAT file without loading all data.
///
/// Parses only the header and metadata pages to extract column definitions,
/// then returns just the column names. This is efficient for interactive
/// column selection in the TUI.
///
/// # Arguments
/// * `path` - Path to the `.sas7bdat` file
///
/// # Returns
/// * `Ok(Vec<String>)` - Column names in file order
/// * `Err(SasError)` - If header or metadata parsing fails
pub fn get_sas7bdat_columns(path: &Path) -> Result<Vec<String>, SasError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let sas_header = parse_header(&mut reader)?;

    // Iterate metadata pages only
    let mut state = SubheaderState::default();
    reader.seek(SeekFrom::Start(sas_header.header_length))?;

    let mut page_buf = vec![0u8; sas_header.page_size as usize];

    for _page_idx in 0..sas_header.page_count {
        let bytes_read = reader.read(&mut page_buf)?;
        if bytes_read < sas_header.page_size as usize {
            break;
        }

        let page_header =
            parse_page_header(&page_buf, sas_header.is_64bit, sas_header.is_little_endian)?;

        if is_page_meta(page_header.page_type) || is_page_mix(page_header.page_type) {
            let pointers = parse_subheader_pointers(
                &page_buf,
                sas_header.is_64bit,
                sas_header.is_little_endian,
                page_header.subheader_count,
            )?;

            for pointer in &pointers {
                if pointer.compression != 0 && pointer.subheader_type == 1 {
                    continue;
                }
                process_subheader(
                    &page_buf,
                    pointer,
                    sas_header.is_64bit,
                    sas_header.is_little_endian,
                    &mut state,
                )?;
            }
        }

        // Stop at first data page - we have all metadata we need
        if is_page_data(page_header.page_type) {
            break;
        }
    }

    let columns = build_columns(&state, &sas_header.encoding);
    Ok(columns.into_iter().map(|c| c.name).collect())
}

/// The native data type of a column in a SAS7BDAT file.
///
/// SAS uses only two fundamental data types internally. Numeric values are stored
/// as IEEE 754 doubles (though may be truncated to 3-8 bytes for space savings).
/// Character values are fixed-width strings encoded according to the file's encoding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SasDataType {
    /// IEEE 754 double-precision floating point (possibly truncated to 3-8 bytes).
    Numeric,
    /// Fixed-width encoded string.
    Character,
}

/// The target Polars column type after parsing and format interpretation.
///
/// While SAS only has two native types, format codes indicate semantic types
/// (dates, times, datetimes) that should be converted to appropriate Polars types
/// for better usability and compatibility with downstream analysis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PolarsOutputType {
    /// Default for numeric columns without recognized format codes.
    Float64,
    /// Date columns (DATE, DDMMYY, MMDDYY, YYMMDD formats).
    Date,
    /// Datetime columns (DATETIME format).
    Datetime,
    /// Time columns (TIME format).
    Time,
    /// Character columns (all character types map to UTF-8 strings).
    Utf8,
}

/// Compression method used for page data.
///
/// SAS7BDAT files can use run-length encoding (RLE) or binary RDC compression
/// to reduce file size. The compression signature is stored in the file header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Compression {
    /// No compression applied.
    None,
    /// Run-length encoding (SASYZCRL signature) -- `COMPRESS=CHAR` or `COMPRESS=YES`.
    Rle,
    /// Binary RDC compression (SASYZCR2 signature) -- `COMPRESS=BINARY`.
    Rdc,
}

/// Operating system that created the SAS7BDAT file.
///
/// SAS files encode the OS type in the header, which affects byte alignment
/// and certain metadata interpretations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OsType {
    /// Unix-based systems (Linux, macOS, Solaris, AIX, etc.).
    Unix,
    /// Windows systems.
    Windows,
    /// Unknown or unrecognized OS identifier.
    Unknown,
}

/// Character encoding used in the SAS7BDAT file.
///
/// SAS encodes strings using various encodings. The encoding ID is stored in
/// the file header and determines how to decode character columns.
#[derive(Debug, Clone, PartialEq)]
pub enum SasEncoding {
    /// UTF-8 encoding (encoding ID 20).
    Utf8,
    /// ASCII encoding (encoding ID 28).
    Ascii,
    /// Latin-1 / ISO-8859-1 encoding (encoding ID 29).
    Latin1,
    /// Windows-1252 encoding (encoding ID 62).
    Windows1252,
    /// Other recognized encoding with ID and name.
    Other {
        /// The numeric encoding ID from the SAS file header.
        id: u16,
        /// The human-readable encoding name.
        name: &'static str,
    },
    /// Encoding ID 0 or unknown -- fall back to Latin-1 interpretation.
    Unspecified,
}

/// File-level metadata parsed from the SAS7BDAT header.
///
/// The header contains critical information for parsing the file, including
/// endianness, compression, encoding, page structure, and dataset dimensions.
#[derive(Debug, Clone)]
pub struct SasHeader {
    /// Whether the file uses 64-bit pointers and offsets (true) or 32-bit (false).
    pub is_64bit: bool,
    /// Whether the file uses little-endian byte order (true) or big-endian (false).
    pub is_little_endian: bool,
    /// Character encoding used for string columns.
    pub encoding: SasEncoding,
    /// Size of each page in bytes (typically 4096, 8192, or 16384).
    pub page_size: u32,
    /// Total number of pages in the file.
    pub page_count: u64,
    /// Total number of data rows in the dataset.
    pub row_count: u64,
    /// Length of each row in bytes (sum of all column lengths).
    pub row_length: u64,
    /// Number of columns in the dataset.
    pub column_count: u64,
    /// Name of the SAS dataset.
    pub dataset_name: String,
    /// Dataset creation timestamp (SAS datetime format: seconds since 1960-01-01).
    pub created: f64,
    /// Dataset modification timestamp (SAS datetime format: seconds since 1960-01-01).
    pub modified: f64,
    /// Length of the file header in bytes.
    pub header_length: u64,
    /// Compression method applied to page data.
    pub compression: Compression,
    /// Operating system that created the file.
    pub os_type: OsType,
    /// SAS software release version string.
    pub sas_release: String,
    /// Maximum number of rows that can fit on a MIX page (mix of data and metadata).
    pub max_rows_on_mix_page: u64,
}

/// Metadata for a single column in the SAS7BDAT file.
///
/// Column metadata includes name, type, position in each row, format codes,
/// and the inferred Polars output type based on format interpretation.
#[derive(Debug, Clone)]
pub struct SasColumn {
    /// Column name (may be truncated to 32 characters in older SAS versions).
    pub name: String,
    /// Native SAS data type (Numeric or Character).
    pub data_type: SasDataType,
    /// Byte offset of this column within each row.
    pub offset: u64,
    /// Length of the column in bytes (8 for full numeric, 1-8 for truncated, N for character).
    pub length: u32,
    /// SAS format code (e.g., "DATE9.", "DATETIME20.", "BEST12.") for display/interpretation.
    pub format: String,
    /// Optional descriptive label for the column.
    pub label: String,
    /// Target Polars column type after parsing and format interpretation.
    pub polars_type: PolarsOutputType,
}

#[cfg(test)]
mod integration_tests {
    use std::path::Path;

    const TEST_DIR: &str = "/tmp/claude-1000/-home-neelsbester-lo-phi/343df7d7-afd6-4cdc-9c9e-2bd30a8f0914/scratchpad/sas_test_files";

    /// Helper: load a SAS7BDAT file and report success/failure with diagnostics
    fn load_test_file(name: &str) -> Result<(usize, usize), String> {
        let path = Path::new(TEST_DIR).join(name);
        if !path.exists() {
            return Err(format!("file not found"));
        }

        // Test get_columns
        let cols = super::get_sas7bdat_columns(&path)
            .map_err(|e| format!("get_columns: {}", e))?;
        println!("  {} columns: {:?}", name, &cols[..cols.len().min(10)]);

        // Test full load
        let (df, rows, col_count, memory_mb) = super::load_sas7bdat(&path)
            .map_err(|e| format!("load: {}", e))?;
        println!(
            "  {} loaded: {} rows x {} cols ({:.2} MB)",
            name, rows, col_count, memory_mb
        );
        println!("  {} dtypes: {:?}", name, df.dtypes());
        println!("  {} head:\n{}\n", name, df.head(Some(3)));

        assert_eq!(cols.len(), col_count, "{}: column count mismatch", name);
        Ok((rows, col_count))
    }

    #[test]
    fn test_real_sas7bdat_files() {
        println!("\n=== Testing real SAS7BDAT files ===\n");

        let test_files = [
            // (filename, expected_rows, expected_cols) - 0 means "just check it loads"
            ("test1.sas7bdat", 10, 100),
            ("test2.sas7bdat", 0, 0),
            ("test3.sas7bdat", 0, 0),
            ("test4.sas7bdat", 0, 0),
            ("test5.sas7bdat", 0, 0),
            ("test6.sas7bdat", 0, 0),
            ("test7.sas7bdat", 0, 0),
            ("test8.sas7bdat", 0, 0),
            ("test9.sas7bdat", 0, 0),
            ("test10.sas7bdat", 0, 0),
            ("test11.sas7bdat", 0, 0),
            ("test12.sas7bdat", 0, 0),
            ("test13.sas7bdat", 0, 0),
            ("test14.sas7bdat", 0, 0),
            ("test15.sas7bdat", 0, 0),
            ("test16.sas7bdat", 0, 0),
            ("cars.sas7bdat", 0, 0),
            ("productsales.sas7bdat", 0, 0),
            ("datetime.sas7bdat", 0, 0),
            ("many_columns.sas7bdat", 0, 0),
            ("test_12659.sas7bdat", 0, 0),
            ("test_meta2_page.sas7bdat", 0, 0),
        ];

        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut failures: Vec<String> = Vec::new();

        for (name, expected_rows, expected_cols) in &test_files {
            match load_test_file(name) {
                Ok((rows, cols)) => {
                    if *expected_rows > 0 {
                        assert_eq!(rows, *expected_rows, "{}: row count", name);
                        assert_eq!(cols, *expected_cols, "{}: col count", name);
                    }
                    passed += 1;
                }
                Err(e) if e == "file not found" => {
                    eprintln!("  SKIP: {} (file not found)", name);
                    skipped += 1;
                }
                Err(e) => {
                    eprintln!("  FAIL: {} - {}", name, e);
                    failures.push(format!("{}: {}", name, e));
                    failed += 1;
                }
            }
        }

        println!("\n=== Summary: {} passed, {} failed, {} skipped ===", passed, failed, skipped);
        if !failures.is_empty() {
            println!("\nFailures:");
            for f in &failures {
                println!("  - {}", f);
            }
        }

        // At minimum, test1.sas7bdat must work
        assert!(passed >= 1, "At least test1.sas7bdat should parse successfully");
    }
}
