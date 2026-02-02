//! SAS7BDAT column metadata construction.
//!
//! This module provides functionality to build the final column list
//! from accumulated subheader state, including name extraction, type
//! mapping, and format interpretation to derive Polars output types.

use super::subheader::SubheaderState;
use super::{PolarsOutputType, SasColumn, SasEncoding};

/// Builds the final column list from accumulated subheader state.
///
/// # Arguments
/// * `state` - Accumulated subheader state from parsing metadata pages
/// * `encoding` - Character encoding for decoding column names
///
/// # Returns
/// * `Vec<SasColumn>` - List of column metadata structs
///
/// # Implementation Notes
/// - Columns are ordered by their appearance in the subheader entries
/// - If any entry list is shorter than others, we use the minimum length
/// - Format parsing determines Polars output type (Date, Datetime, Time, Float64, Utf8)
pub fn build_columns(state: &SubheaderState, encoding: &SasEncoding) -> Vec<SasColumn> {
    let num_columns = state
        .column_name_entries
        .len()
        .min(state.column_attr_entries.len());

    let mut columns = Vec::with_capacity(num_columns);

    for i in 0..num_columns {
        let name_entry = &state.column_name_entries[i];
        let attr_entry = &state.column_attr_entries[i];

        // Extract name from text blocks
        let name = extract_text_from_blocks(
            &state.column_text_blocks,
            name_entry.text_index as usize,
            name_entry.offset as usize,
            name_entry.length as usize,
            encoding,
        );

        // Extract format and label (if available)
        let (format, label) = if i < state.column_format_entries.len() {
            let format_entry = &state.column_format_entries[i];
            let format = extract_text_from_blocks(
                &state.column_text_blocks,
                format_entry.format_text_index as usize,
                format_entry.format_offset as usize,
                format_entry.format_length as usize,
                encoding,
            );
            let label = extract_text_from_blocks(
                &state.column_text_blocks,
                format_entry.label_text_index as usize,
                format_entry.label_offset as usize,
                format_entry.label_length as usize,
                encoding,
            );
            (format, label)
        } else {
            (String::new(), String::new())
        };

        // Determine Polars output type based on format and data type
        let polars_type = infer_polars_type(&format, &attr_entry.data_type);

        columns.push(SasColumn {
            name,
            data_type: attr_entry.data_type,
            offset: attr_entry.offset,
            length: attr_entry.length,
            format,
            label,
            polars_type,
        });
    }

    columns
}

/// Extracts a text string from column text blocks.
///
/// # Arguments
/// * `text_blocks` - All text blocks accumulated from ColumnText subheaders
/// * `text_index` - Index of the text block to use
/// * `offset` - Byte offset within the text block
/// * `length` - Length of the text in bytes
/// * `encoding` - Character encoding for decoding
///
/// # Returns
/// * `String` - Decoded text, or empty string if extraction fails
fn extract_text_from_blocks(
    text_blocks: &[Vec<u8>],
    text_index: usize,
    offset: usize,
    length: usize,
    encoding: &SasEncoding,
) -> String {
    if length == 0 {
        return String::new();
    }

    if text_index >= text_blocks.len() {
        return String::new();
    }

    let block = &text_blocks[text_index];
    if offset + length > block.len() {
        return String::new();
    }

    let bytes = &block[offset..offset + length];
    decode_text(bytes, encoding)
}

/// Decodes bytes to string using the specified encoding.
///
/// # Arguments
/// * `bytes` - Raw bytes to decode
/// * `encoding` - Character encoding
///
/// # Returns
/// * `String` - Decoded text (lossy conversion for unsupported encodings)
fn decode_text(bytes: &[u8], encoding: &SasEncoding) -> String {
    match encoding {
        SasEncoding::Utf8 => String::from_utf8_lossy(bytes).trim().to_string(),
        SasEncoding::Ascii => String::from_utf8_lossy(bytes).trim().to_string(),
        SasEncoding::Latin1 => decode_latin1(bytes).trim().to_string(),
        SasEncoding::Windows1252 => decode_windows1252(bytes).trim().to_string(),
        SasEncoding::Other { .. } => String::from_utf8_lossy(bytes).trim().to_string(),
        SasEncoding::Unspecified => decode_latin1(bytes).trim().to_string(), // Default to Latin-1
    }
}

/// Decodes Latin-1 (ISO-8859-1) bytes to String.
fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Decodes Windows-1252 bytes to String.
///
/// For simplicity, we use Latin-1 decoding as a fallback. A full Windows-1252
/// implementation would map the 0x80-0x9F range to specific Unicode characters.
fn decode_windows1252(bytes: &[u8]) -> String {
    // Simplified: treat as Latin-1 for now
    // TODO: Full Windows-1252 mapping for 0x80-0x9F range
    decode_latin1(bytes)
}

/// Infers the Polars output type based on SAS format string and data type.
///
/// # Arguments
/// * `format` - SAS format string (e.g., "DATE9.", "DATETIME20.", "BEST12.")
/// * `data_type` - Native SAS data type (Numeric or Character)
///
/// # Returns
/// * `PolarsOutputType` - Target Polars column type
///
/// # Format Parsing Rules
/// - Strip trailing digits and '.' (e.g., DATE9. → DATE)
/// - Case-insensitive matching
/// - DATE, DDMMYY, MMDDYY, YYMMDD → Date
/// - DATETIME → Datetime
/// - TIME → Time
/// - Character type → Utf8
/// - Everything else for Numeric → Float64
fn infer_polars_type(format: &str, data_type: &super::SasDataType) -> PolarsOutputType {
    use super::SasDataType;

    // Character columns always map to Utf8
    if *data_type == SasDataType::Character {
        return PolarsOutputType::Utf8;
    }

    // Strip trailing digits and '.' from format
    let clean_format = format
        .trim()
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
        .to_uppercase();

    // Match format to output type
    if clean_format.is_empty() {
        return PolarsOutputType::Float64;
    }

    if clean_format == "DATE"
        || clean_format == "DDMMYY"
        || clean_format == "MMDDYY"
        || clean_format == "YYMMDD"
        || clean_format == "YYMMDDD"
        || clean_format == "JULIAN"
    {
        PolarsOutputType::Date
    } else if clean_format == "DATETIME" {
        PolarsOutputType::Datetime
    } else if clean_format == "TIME" {
        PolarsOutputType::Time
    } else {
        PolarsOutputType::Float64
    }
}

#[cfg(test)]
mod tests {
    use super::super::SasDataType;
    use super::*;

    #[test]
    fn test_decode_latin1() {
        let bytes = vec![0x48, 0x65, 0x6C, 0x6C, 0x6F]; // "Hello"
        assert_eq!(decode_latin1(&bytes), "Hello");

        let bytes = vec![0xE9]; // é in Latin-1
        assert_eq!(decode_latin1(&bytes), "é");
    }

    #[test]
    fn test_decode_text_utf8() {
        let bytes = b"Hello UTF-8";
        let result = decode_text(bytes, &SasEncoding::Utf8);
        assert_eq!(result, "Hello UTF-8");
    }

    #[test]
    fn test_decode_text_latin1() {
        let bytes = vec![0x48, 0x65, 0x6C, 0x6C, 0x6F]; // "Hello"
        let result = decode_text(&bytes, &SasEncoding::Latin1);
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_infer_polars_type_date_formats() {
        assert_eq!(
            infer_polars_type("DATE9.", &SasDataType::Numeric),
            PolarsOutputType::Date
        );
        assert_eq!(
            infer_polars_type("DDMMYY10.", &SasDataType::Numeric),
            PolarsOutputType::Date
        );
        assert_eq!(
            infer_polars_type("MMDDYY8.", &SasDataType::Numeric),
            PolarsOutputType::Date
        );
        assert_eq!(
            infer_polars_type("YYMMDD.", &SasDataType::Numeric),
            PolarsOutputType::Date
        );
    }

    #[test]
    fn test_infer_polars_type_datetime() {
        assert_eq!(
            infer_polars_type("DATETIME20.", &SasDataType::Numeric),
            PolarsOutputType::Datetime
        );
        assert_eq!(
            infer_polars_type("DATETIME.", &SasDataType::Numeric),
            PolarsOutputType::Datetime
        );
    }

    #[test]
    fn test_infer_polars_type_time() {
        assert_eq!(
            infer_polars_type("TIME8.", &SasDataType::Numeric),
            PolarsOutputType::Time
        );
        assert_eq!(
            infer_polars_type("TIME.", &SasDataType::Numeric),
            PolarsOutputType::Time
        );
    }

    #[test]
    fn test_infer_polars_type_numeric_default() {
        assert_eq!(
            infer_polars_type("BEST12.", &SasDataType::Numeric),
            PolarsOutputType::Float64
        );
        assert_eq!(
            infer_polars_type("F8.2", &SasDataType::Numeric),
            PolarsOutputType::Float64
        );
        assert_eq!(
            infer_polars_type("", &SasDataType::Numeric),
            PolarsOutputType::Float64
        );
    }

    #[test]
    fn test_infer_polars_type_character() {
        assert_eq!(
            infer_polars_type("$CHAR20.", &SasDataType::Character),
            PolarsOutputType::Utf8
        );
        assert_eq!(
            infer_polars_type("", &SasDataType::Character),
            PolarsOutputType::Utf8
        );
    }

    #[test]
    fn test_infer_polars_type_case_insensitive() {
        assert_eq!(
            infer_polars_type("date9.", &SasDataType::Numeric),
            PolarsOutputType::Date
        );
        assert_eq!(
            infer_polars_type("DaTeTiMe20.", &SasDataType::Numeric),
            PolarsOutputType::Datetime
        );
    }

    #[test]
    fn test_extract_text_empty_length() {
        let blocks = vec![vec![b'H', b'e', b'l', b'l', b'o']];
        let result = extract_text_from_blocks(&blocks, 0, 0, 0, &SasEncoding::Utf8);
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_text_valid() {
        let blocks = vec![vec![
            b'H', b'e', b'l', b'l', b'o', b' ', b'W', b'o', b'r', b'l', b'd',
        ]];
        let result = extract_text_from_blocks(&blocks, 0, 0, 5, &SasEncoding::Utf8);
        assert_eq!(result, "Hello");

        let result = extract_text_from_blocks(&blocks, 0, 6, 5, &SasEncoding::Utf8);
        assert_eq!(result, "World");
    }

    #[test]
    fn test_extract_text_out_of_bounds() {
        let blocks = vec![vec![b'H', b'e', b'l', b'l', b'o']];
        let result = extract_text_from_blocks(&blocks, 0, 0, 100, &SasEncoding::Utf8);
        assert_eq!(result, "");

        let result = extract_text_from_blocks(&blocks, 10, 0, 5, &SasEncoding::Utf8);
        assert_eq!(result, "");
    }
}
