//! Data extraction and conversion from SAS7BDAT pages to Polars Series.
//!
//! This module handles:
//! - Extracting rows from data pages, mix pages, and compressed pages
//! - Converting numeric values (full 8-byte and truncated 3-7 byte)
//! - Detecting SAS missing value sentinels
//! - Converting SAS dates/datetimes/times to Unix epoch-based representations
//! - Decoding character columns with various encodings
//! - Building Polars Series with appropriate dtypes

use super::constants::*;
use super::decompress::{decompress_rdc, decompress_rle};
use super::error::SasError;
use super::page::{is_page_comp, is_page_data, is_page_mix, parse_page_header};
use super::{Compression, PolarsOutputType, SasColumn, SasDataType, SasEncoding, SasHeader};
use polars::prelude::*;

/// Represents a single value extracted from a SAS7BDAT row.
///
/// This enum captures the various Polars-compatible types that SAS columns
/// can be converted to, including null values.
#[derive(Debug, Clone)]
pub enum ColumnValue {
    /// Numeric column stored as Float64.
    Float64(f64),
    /// Date column stored as days since Unix epoch (1970-01-01).
    Int32(i32),
    /// Datetime or Time column stored as milliseconds or nanoseconds since epoch.
    Int64(i64),
    /// Character column stored as UTF-8 string.
    Utf8(String),
    /// Missing/null value.
    Null,
}

/// Extracts all rows from a single page.
///
/// Handles data pages, mix pages, and compressed pages. Returns a vector of rows,
/// where each row is a vector of `ColumnValue`s corresponding to the columns.
///
/// # Arguments
///
/// * `page_data` - Raw bytes for the entire page
/// * `header` - File-level metadata (endianness, compression, dimensions)
/// * `columns` - Column metadata for value extraction
/// * `page_index` - Zero-based page index (for error reporting)
/// * `compression` - Compression method (None, RLE, or RDC)
/// * `rows_collected` - Number of rows already collected from previous pages
/// * `total_rows` - Total row count from file header (to avoid over-reading)
///
/// # Returns
///
/// * `Ok(Vec<Vec<ColumnValue>>)` - Extracted rows
/// * `Err(SasError)` - If page parsing, decompression, or value extraction fails
///
/// # Page Types
///
/// - **Data pages**: Rows start after page header (offset 24 for 64-bit, 8 for 32-bit)
/// - **Mix pages**: Rows start after subheader pointer table
/// - **Compressed pages**: Decompress first, then extract as data page
#[allow(dead_code)]
pub fn extract_rows_from_page(
    page_data: &[u8],
    header: &SasHeader,
    columns: &[SasColumn],
    page_index: u64,
    compression: Compression,
    rows_collected: u64,
    total_rows: u64,
) -> Result<Vec<Vec<ColumnValue>>, SasError> {
    // Parse page header
    let page_header = parse_page_header(page_data, header.is_64bit, header.is_little_endian)?;

    // Handle compressed pages first
    let decompressed_data;
    let data_to_use = if is_page_comp(page_header.page_type) {
        // Compressed page - decompress based on compression type
        let output_length = header.page_size as usize;
        decompressed_data = match compression {
            Compression::Rle => decompress_rle(page_data, output_length).map_err(|e| {
                SasError::DecompressionError {
                    page_index,
                    message: e.to_string(),
                }
            })?,
            Compression::Rdc => decompress_rdc(page_data, output_length).map_err(|e| {
                SasError::DecompressionError {
                    page_index,
                    message: e.to_string(),
                }
            })?,
            Compression::None => {
                return Err(SasError::DecompressionError {
                    page_index,
                    message: "Page marked as compressed but compression is None".to_string(),
                });
            }
        };
        &decompressed_data[..]
    } else {
        page_data
    };

    // Determine data start offset and row count based on page type
    // page_bit_offset accounts for the page prefix bytes before the page header fields
    let page_bit_offset: usize = if header.is_64bit { 32 } else { 16 };
    let page_header_size = page_bit_offset + 8; // prefix + type(2) + block_count(2) + subheader_count(2) + pad(2)

    let (data_start, rows_on_page) = if is_page_data(page_header.page_type) {
        // Data page: rows start after page header (no subheader pointers)
        let rows = page_header.block_count as u64;
        (page_header_size, rows)
    } else if is_page_mix(page_header.page_type) {
        // Mix page: rows start after subheader pointer table, aligned to 8 bytes
        let pointer_size: usize = if header.is_64bit { 24 } else { 12 };
        let raw_offset = page_header_size + (page_header.subheader_count as usize * pointer_size);
        // Align to 8-byte boundary (required by SAS7BDAT spec)
        let offset = (raw_offset + 7) & !7;

        // Number of data rows = block_count - subheader_count, capped by max_rows_on_mix_page
        let data_blocks = page_header
            .block_count
            .saturating_sub(page_header.subheader_count);
        let rows = std::cmp::min(data_blocks as u64, header.max_rows_on_mix_page);
        (offset, rows)
    } else {
        // Metadata-only page, no data rows
        return Ok(Vec::new());
    };

    // Don't exceed total row count
    let remaining_rows = total_rows.saturating_sub(rows_collected);
    let rows_to_extract = std::cmp::min(rows_on_page, remaining_rows);

    // Extract rows
    let mut rows = Vec::with_capacity(rows_to_extract as usize);
    let row_length = header.row_length as usize;

    for row_idx in 0..rows_to_extract {
        let row_offset = data_start + (row_idx as usize * row_length);

        // Ensure we don't read past the page boundary
        if row_offset + row_length > data_to_use.len() {
            break;
        }

        let row_data = &data_to_use[row_offset..row_offset + row_length];
        let row_values =
            extract_row_values(row_data, columns, &header.encoding, header.is_little_endian)?;
        rows.push(row_values);
    }

    Ok(rows)
}

/// Extracts values for all columns from a single row.
///
/// # Arguments
///
/// * `row_data` - Raw bytes for the row (length = `header.row_length`)
/// * `columns` - Column metadata (offset, length, type)
/// * `encoding` - Character encoding for string columns
/// * `is_little_endian` - Byte order for numeric columns
///
/// # Returns
///
/// * `Ok(Vec<ColumnValue>)` - One value per column
/// * `Err(SasError)` - If value extraction fails
fn extract_row_values(
    row_data: &[u8],
    columns: &[SasColumn],
    encoding: &SasEncoding,
    is_little_endian: bool,
) -> Result<Vec<ColumnValue>, SasError> {
    let mut values = Vec::with_capacity(columns.len());

    for col in columns {
        let offset = col.offset as usize;
        let length = col.length as usize;

        // Ensure we don't read past row boundary
        if offset + length > row_data.len() {
            values.push(ColumnValue::Null);
            continue;
        }

        let col_bytes = &row_data[offset..offset + length];

        let value = match col.data_type {
            SasDataType::Numeric => {
                extract_numeric_value(col_bytes, &col.polars_type, is_little_endian)?
            }
            SasDataType::Character => extract_character_value(col_bytes, encoding),
        };

        values.push(value);
    }

    Ok(values)
}

/// Extracts a numeric value from raw bytes.
///
/// Handles:
/// - Zero-length columns (null)
/// - SAS missing value sentinels (standard `.` and special `.A`-`.Z`, `._`)
/// - Truncated numerics (3-7 bytes) with zero-padding
/// - Full 8-byte IEEE 754 doubles
/// - Type conversion to Date, Datetime, Time, or Float64
///
/// # Arguments
///
/// * `bytes` - Raw column bytes (length 0-8)
/// * `polars_type` - Target Polars type
/// * `is_little_endian` - Byte order
///
/// # Returns
///
/// * `Ok(ColumnValue)` - Extracted and converted value
/// * `Err(SasError)` - If numeric conversion fails
fn extract_numeric_value(
    bytes: &[u8],
    polars_type: &PolarsOutputType,
    is_little_endian: bool,
) -> Result<ColumnValue, SasError> {
    // Zero-length numeric = null
    if bytes.is_empty() {
        return Ok(ColumnValue::Null);
    }

    // Check for missing value sentinel
    let mut buf = [0u8; 8];

    if bytes.len() < 8 {
        // Truncated numeric - SAS stores the most-significant bytes of the IEEE 754 double.
        // Zero-pad the least-significant positions to reconstruct the full 8-byte value.
        if is_little_endian {
            // LE layout: byte[0]=LSB, byte[7]=MSB. Stored bytes are the MSBs,
            // so place them at the high end and zero-fill the low end.
            buf[8 - bytes.len()..].copy_from_slice(bytes);
        } else {
            // BE layout: byte[0]=MSB, byte[7]=LSB. Stored bytes are the MSBs,
            // so place them at the low end and zero-fill the high end.
            buf[..bytes.len()].copy_from_slice(bytes);
        }
    } else {
        // Full 8-byte numeric
        buf.copy_from_slice(bytes);
    }

    // Check for SAS missing value
    if is_missing_value(&buf, is_little_endian) {
        return Ok(ColumnValue::Null);
    }

    // Convert to f64
    let value = if is_little_endian {
        f64::from_le_bytes(buf)
    } else {
        f64::from_be_bytes(buf)
    };

    // NaN also treated as null
    if value.is_nan() {
        return Ok(ColumnValue::Null);
    }

    // Convert to target Polars type
    match polars_type {
        PolarsOutputType::Float64 => Ok(ColumnValue::Float64(value)),
        PolarsOutputType::Date => {
            // SAS date: days since 1960-01-01
            // Unix date: days since 1970-01-01
            let unix_days = (value as i64) - SAS_EPOCH_OFFSET_DAYS;
            Ok(ColumnValue::Int32(unix_days as i32))
        }
        PolarsOutputType::Datetime => {
            // SAS datetime: seconds since 1960-01-01 00:00:00
            // Unix datetime: milliseconds since 1970-01-01 00:00:00
            let unix_seconds = value - (SAS_EPOCH_OFFSET_SECONDS as f64);
            let unix_ms = (unix_seconds * MS_PER_SECOND as f64) as i64;
            Ok(ColumnValue::Int64(unix_ms))
        }
        PolarsOutputType::Time => {
            // SAS time: seconds since midnight
            // Polars time: nanoseconds since midnight
            let ns = (value * NS_PER_SECOND as f64) as i64;
            Ok(ColumnValue::Int64(ns))
        }
        PolarsOutputType::Utf8 => {
            // Should not happen for numeric columns
            Err(SasError::NumericError {
                column: "unknown".to_string(),
                row: 0,
                message: "Numeric column mapped to Utf8 type".to_string(),
            })
        }
    }
}

/// Extracts a character value from raw bytes.
///
/// Decodes the byte slice using the file's encoding, trims trailing spaces,
/// and returns null for empty strings.
///
/// # Arguments
///
/// * `bytes` - Raw column bytes
/// * `encoding` - File character encoding
///
/// # Returns
///
/// * `ColumnValue::Utf8(String)` - Decoded string
/// * `ColumnValue::Null` - If empty after trimming
fn extract_character_value(bytes: &[u8], encoding: &SasEncoding) -> ColumnValue {
    let decoded = match encoding {
        SasEncoding::Utf8 | SasEncoding::Ascii => String::from_utf8_lossy(bytes).into_owned(),
        SasEncoding::Latin1 | SasEncoding::Unspecified => {
            // Use Windows-1252 which is a superset of Latin-1
            encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
        }
        SasEncoding::Windows1252 => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
        SasEncoding::Other { name, .. } => {
            // Try to find the encoding by name
            if let Some(enc) = encoding_rs::Encoding::for_label(name.as_bytes()) {
                enc.decode(bytes).0.into_owned()
            } else {
                // Fallback to UTF-8 lossy
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
    };

    // Trim trailing spaces (SAS pads character columns)
    let trimmed = decoded.trim_end();

    if trimmed.is_empty() {
        ColumnValue::Null
    } else {
        ColumnValue::Utf8(trimmed.to_string())
    }
}

/// Builds a Polars Series from a vector of ColumnValue.
///
/// Converts the generic `ColumnValue` enum into a strongly-typed Polars Series
/// with the appropriate dtype based on `polars_type`.
///
/// # Arguments
///
/// * `name` - Column name
/// * `polars_type` - Target Polars dtype
/// * `values` - Vector of column values
///
/// # Returns
///
/// * `Ok(Series)` - Polars Series with correct dtype
/// * `Err(SasError)` - If type conversion fails
#[allow(dead_code)]
pub fn build_series_from_column_values(
    name: &str,
    polars_type: &PolarsOutputType,
    values: Vec<ColumnValue>,
) -> Result<Series, SasError> {
    match polars_type {
        PolarsOutputType::Float64 => {
            let ca: Float64Chunked = values
                .iter()
                .map(|v| match v {
                    ColumnValue::Float64(f) => Some(*f),
                    ColumnValue::Null => None,
                    _ => None,
                })
                .collect();
            Ok(ca.with_name(name.into()).into_series())
        }
        PolarsOutputType::Date => {
            let ca: Int32Chunked = values
                .iter()
                .map(|v| match v {
                    ColumnValue::Int32(d) => Some(*d),
                    ColumnValue::Null => None,
                    _ => None,
                })
                .collect();
            let series = ca.with_name(name.into()).into_series();
            // Cast to Date dtype
            series
                .cast(&DataType::Date)
                .map_err(|e| SasError::NumericError {
                    column: name.to_string(),
                    row: 0,
                    message: format!("Failed to cast to Date: {}", e),
                })
        }
        PolarsOutputType::Datetime => {
            let ca: Int64Chunked = values
                .iter()
                .map(|v| match v {
                    ColumnValue::Int64(dt) => Some(*dt),
                    ColumnValue::Null => None,
                    _ => None,
                })
                .collect();
            let series = ca.with_name(name.into()).into_series();
            // Cast to Datetime dtype (milliseconds, no timezone)
            series
                .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
                .map_err(|e| SasError::NumericError {
                    column: name.to_string(),
                    row: 0,
                    message: format!("Failed to cast to Datetime: {}", e),
                })
        }
        PolarsOutputType::Time => {
            let ca: Int64Chunked = values
                .iter()
                .map(|v| match v {
                    ColumnValue::Int64(t) => Some(*t),
                    ColumnValue::Null => None,
                    _ => None,
                })
                .collect();
            let series = ca.with_name(name.into()).into_series();
            // Cast to Time dtype (nanoseconds)
            series
                .cast(&DataType::Time)
                .map_err(|e| SasError::NumericError {
                    column: name.to_string(),
                    row: 0,
                    message: format!("Failed to cast to Time: {}", e),
                })
        }
        PolarsOutputType::Utf8 => {
            let ca: StringChunked = values
                .iter()
                .map(|v| match v {
                    ColumnValue::Utf8(s) => Some(s.as_str()),
                    ColumnValue::Null => None,
                    _ => None,
                })
                .collect();
            Ok(ca.with_name(name.into()).into_series())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncated_numeric_le() {
        // SAS truncated numerics store the most-significant bytes.
        // For LE, these are placed at the high end (right-aligned).
        // Example: 8.0 as full LE f64 = [00, 00, 00, 00, 00, 00, 20, 40]
        // Truncated to 3 bytes: [00, 20, 40] (the 3 MSBs)
        // Reconstructed: [00, 00, 00, 00, 00, 00, 20, 40]
        let bytes = [0x00u8, 0x20, 0x40];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, true).unwrap();
        match result {
            ColumnValue::Float64(f) => assert_eq!(f, 8.0),
            _ => panic!("Expected Float64, got {:?}", result),
        }

        // 42.0 as full LE f64 = [00, 00, 00, 00, 00, 00, 45, 40]
        // Truncated to 3 bytes: [00, 45, 40] (the 3 MSBs at positions 5,6,7)
        // Reconstructed: [00, 00, 00, 00, 00, 00, 45, 40] = 42.0 (exact, no info lost)
        let bytes = [0x00u8, 0x45, 0x40];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, true).unwrap();
        match result {
            ColumnValue::Float64(f) => assert_eq!(f, 42.0),
            _ => panic!("Expected Float64, got {:?}", result),
        }

        // 5-byte truncation of 1234.0 = [00, 00, 00, 4A, 93, 40, 00, 00] -> wait
        // 1234.0 as LE f64: [00, 00, 00, 00, 00, 4A, 93, 40]
        // Truncated to 5 bytes: [00, 00, 4A, 93, 40]
        let bytes = [0x00u8, 0x00, 0x4A, 0x93, 0x40];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, true).unwrap();
        match result {
            ColumnValue::Float64(f) => {
                let expected = f64::from_le_bytes([0x00, 0x00, 0x00, 0x00, 0x00, 0x4A, 0x93, 0x40]);
                assert_eq!(f, expected);
            }
            _ => panic!("Expected Float64, got {:?}", result),
        }
    }

    #[test]
    fn test_truncated_numeric_be() {
        // For BE, most-significant bytes are placed at the low end (left-aligned).
        // 8.0 as full BE f64 = [40, 20, 00, 00, 00, 00, 00, 00]
        // Truncated to 3 bytes: [40, 20, 00]
        // Reconstructed: [40, 20, 00, 00, 00, 00, 00, 00]
        let bytes = [0x40u8, 0x20, 0x00];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, false).unwrap();
        match result {
            ColumnValue::Float64(f) => assert_eq!(f, 8.0),
            _ => panic!("Expected Float64, got {:?}", result),
        }

        // 4-byte numeric: 1.0 as BE = [3F, F0, 00, 00, 00, 00, 00, 00]
        // Truncated to 4 bytes: [3F, F0, 00, 00]
        let bytes = [0x3Fu8, 0xF0, 0x00, 0x00];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, false).unwrap();
        match result {
            ColumnValue::Float64(f) => assert_eq!(f, 1.0),
            _ => panic!("Expected Float64, got {:?}", result),
        }

        // 5-byte numeric: 1234.0 as BE = [40, 93, 4A, 00, 00, 00, 00, 00]
        // Truncated to 5 bytes: [40, 93, 4A, 00, 00]
        let bytes = [0x40u8, 0x93, 0x4A, 0x00, 0x00];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, false).unwrap();
        match result {
            ColumnValue::Float64(f) => {
                let expected = f64::from_be_bytes([0x40, 0x93, 0x4A, 0x00, 0x00, 0x00, 0x00, 0x00]);
                assert_eq!(f, expected);
            }
            _ => panic!("Expected Float64, got {:?}", result),
        }
    }

    #[test]
    fn test_missing_value_detection() {
        // Standard missing: 0x2E at first byte (LE)
        let bytes = [0x2Eu8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, true).unwrap();
        assert!(matches!(result, ColumnValue::Null));

        // Letter missing: .A = 0x41 at first byte (LE)
        let bytes = [0x41u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, true).unwrap();
        assert!(matches!(result, ColumnValue::Null));

        // Underscore missing: ._ = 0x5F at first byte (LE)
        let bytes = [0x5Fu8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, true).unwrap();
        assert!(matches!(result, ColumnValue::Null));

        // Standard missing in big-endian: 0x2E at last byte
        let bytes = [0xF0u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2E];
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Float64, false).unwrap();
        assert!(matches!(result, ColumnValue::Null));
    }

    #[test]
    fn test_date_conversion() {
        // SAS date 0 = 1960-01-01
        // Unix epoch 0 = 1970-01-01
        // Offset = 3653 days

        // SAS date 3653 should map to Unix epoch day 0
        let sas_date: f64 = 3653.0;
        let bytes = sas_date.to_le_bytes();
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Date, true).unwrap();
        match result {
            ColumnValue::Int32(d) => assert_eq!(d, 0),
            _ => panic!("Expected Int32, got {:?}", result),
        }

        // SAS date 0 should map to Unix epoch day -3653
        let sas_date: f64 = 0.0;
        let bytes = sas_date.to_le_bytes();
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Date, true).unwrap();
        match result {
            ColumnValue::Int32(d) => assert_eq!(d, -3653),
            _ => panic!("Expected Int32, got {:?}", result),
        }
    }

    #[test]
    fn test_datetime_conversion() {
        // SAS datetime 0 = 1960-01-01 00:00:00
        // Unix epoch 0 = 1970-01-01 00:00:00
        // Offset = 315,619,200 seconds = 315,619,200,000 milliseconds

        // SAS datetime 315,619,200 should map to Unix epoch 0
        let sas_datetime: f64 = 315_619_200.0;
        let bytes = sas_datetime.to_le_bytes();
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Datetime, true).unwrap();
        match result {
            ColumnValue::Int64(dt) => assert_eq!(dt, 0),
            _ => panic!("Expected Int64, got {:?}", result),
        }

        // SAS datetime 0 should map to negative Unix epoch
        let sas_datetime: f64 = 0.0;
        let bytes = sas_datetime.to_le_bytes();
        let result = extract_numeric_value(&bytes, &PolarsOutputType::Datetime, true).unwrap();
        match result {
            ColumnValue::Int64(dt) => assert_eq!(dt, -315_619_200_000),
            _ => panic!("Expected Int64, got {:?}", result),
        }
    }

    #[test]
    fn test_character_decode_utf8() {
        // UTF-8 string with trailing spaces
        let bytes = b"Hello   ";
        let result = extract_character_value(bytes, &SasEncoding::Utf8);
        match result {
            ColumnValue::Utf8(s) => assert_eq!(s, "Hello"),
            _ => panic!("Expected Utf8, got {:?}", result),
        }
    }

    #[test]
    fn test_character_decode_latin1() {
        // Latin-1 byte 0xE9 = é
        let bytes = &[0xE9u8, b'c', b'o', b'l', b'e', b' ', b' '];
        let result = extract_character_value(bytes, &SasEncoding::Latin1);
        match result {
            ColumnValue::Utf8(s) => assert_eq!(s, "école"),
            _ => panic!("Expected Utf8, got {:?}", result),
        }
    }

    #[test]
    fn test_empty_string_is_null() {
        // All spaces should become null
        let bytes = b"     ";
        let result = extract_character_value(bytes, &SasEncoding::Utf8);
        assert!(matches!(result, ColumnValue::Null));

        // Empty string
        let bytes = b"";
        let result = extract_character_value(bytes, &SasEncoding::Utf8);
        assert!(matches!(result, ColumnValue::Null));
    }

    #[test]
    fn test_build_series_float64() {
        let values = vec![
            ColumnValue::Float64(1.5),
            ColumnValue::Float64(2.5),
            ColumnValue::Null,
            ColumnValue::Float64(3.5),
        ];
        let series =
            build_series_from_column_values("test", &PolarsOutputType::Float64, values).unwrap();
        assert_eq!(series.name(), "test");
        assert_eq!(series.len(), 4);
        assert_eq!(series.dtype(), &DataType::Float64);
    }

    #[test]
    fn test_build_series_date() {
        let values = vec![
            ColumnValue::Int32(0),
            ColumnValue::Int32(100),
            ColumnValue::Null,
        ];
        let series =
            build_series_from_column_values("date_col", &PolarsOutputType::Date, values).unwrap();
        assert_eq!(series.name(), "date_col");
        assert_eq!(series.len(), 3);
        assert_eq!(series.dtype(), &DataType::Date);
    }

    #[test]
    fn test_build_series_datetime() {
        let values = vec![
            ColumnValue::Int64(0),
            ColumnValue::Int64(1000),
            ColumnValue::Null,
        ];
        let series =
            build_series_from_column_values("datetime_col", &PolarsOutputType::Datetime, values)
                .unwrap();
        assert_eq!(series.name(), "datetime_col");
        assert_eq!(series.len(), 3);
        assert_eq!(
            series.dtype(),
            &DataType::Datetime(TimeUnit::Milliseconds, None)
        );
    }

    #[test]
    fn test_build_series_time() {
        let values = vec![
            ColumnValue::Int64(0),
            ColumnValue::Int64(1_000_000_000),
            ColumnValue::Null,
        ];
        let series =
            build_series_from_column_values("time_col", &PolarsOutputType::Time, values).unwrap();
        assert_eq!(series.name(), "time_col");
        assert_eq!(series.len(), 3);
        assert_eq!(series.dtype(), &DataType::Time);
    }

    #[test]
    fn test_build_series_utf8() {
        let values = vec![
            ColumnValue::Utf8("hello".to_string()),
            ColumnValue::Utf8("world".to_string()),
            ColumnValue::Null,
        ];
        let series =
            build_series_from_column_values("str_col", &PolarsOutputType::Utf8, values).unwrap();
        assert_eq!(series.name(), "str_col");
        assert_eq!(series.len(), 3);
        assert_eq!(series.dtype(), &DataType::String);
    }
}
