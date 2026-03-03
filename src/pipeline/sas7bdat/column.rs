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
/// Uses `encoding_rs` for all non-UTF-8 encodings, consistent with how
/// `data.rs` decodes character column values.
///
/// # Arguments
/// * `bytes` - Raw bytes to decode
/// * `encoding` - Character encoding
///
/// # Returns
/// * `String` - Decoded text, trimmed of surrounding whitespace
fn decode_text(bytes: &[u8], encoding: &SasEncoding) -> String {
    let decoded = match encoding {
        SasEncoding::Utf8 | SasEncoding::Ascii => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        SasEncoding::Latin1 | SasEncoding::Unspecified | SasEncoding::Windows1252 => {
            // Windows-1252 is a superset of Latin-1; use it for all three variants
            encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
        }
        SasEncoding::Other { name, .. } => {
            if let Some(enc) = encoding_rs::Encoding::for_label(name.as_bytes()) {
                enc.decode(bytes).0.into_owned()
            } else {
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
    };
    decoded.trim().to_string()
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
/// - Date formats: DATE, DDMMYY, MMDDYY, YYMMDD, YYMMDDD, JULIAN, MONYY, YYMON,
///   MONNAME, WEEKDATE, WEEKDAY, QTR, YEAR, E8601DA, B8601DA, EURDFDD
/// - Datetime formats: DATETIME, DTDATE, DTMONYY, DTWKDATX, E8601DT, B8601DT,
///   NLDATM, DATEAMPM; also any format starting with "DT"
/// - Time formats: TIME, TOD, HHMM, MMSS, E8601TM, B8601TM, TIMEAMPM, HOUR
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

    if clean_format.is_empty() {
        return PolarsOutputType::Float64;
    }

    // --- Datetime formats (check before Date to avoid "DATETIME" matching Date) ---
    const DATETIME_FORMATS: &[&str] = &[
        "DATETIME",
        "DTDATE",
        "DTMONYY",
        "DTWKDATX",
        "E8601DT",
        "B8601DT",
        "NLDATM",
        "DATEAMPM",
    ];
    // Any format starting with "DT" is a datetime
    if clean_format.starts_with("DT")
        || DATETIME_FORMATS.contains(&clean_format.as_str())
    {
        return PolarsOutputType::Datetime;
    }

    // --- Date formats ---
    const DATE_FORMATS: &[&str] = &[
        "DATE",
        "DDMMYY",
        "MMDDYY",
        "YYMMDD",
        "YYMMDDD",
        "JULIAN",
        "MONYY",
        "YYMON",
        "MONNAME",
        "WEEKDATE",
        "WEEKDAY",
        "QTR",
        "YEAR",
        "E8601DA",
        "B8601DA",
        "EURDFDD",
    ];
    if DATE_FORMATS.contains(&clean_format.as_str()) {
        return PolarsOutputType::Date;
    }

    // --- Time formats ---
    const TIME_FORMATS: &[&str] = &[
        "TIME",
        "TOD",
        "HHMM",
        "MMSS",
        "E8601TM",
        "B8601TM",
        "TIMEAMPM",
        "HOUR",
    ];
    if TIME_FORMATS.contains(&clean_format.as_str()) {
        return PolarsOutputType::Time;
    }

    PolarsOutputType::Float64
}

#[cfg(test)]
mod tests {
    use super::super::SasDataType;
    use super::*;

    #[test]
    fn test_decode_text_utf8() {
        let bytes = b"Hello UTF-8";
        let result = decode_text(bytes, &SasEncoding::Utf8);
        assert_eq!(result, "Hello UTF-8");
    }

    #[test]
    fn test_decode_text_ascii() {
        let bytes = b"Hello ASCII";
        let result = decode_text(bytes, &SasEncoding::Ascii);
        assert_eq!(result, "Hello ASCII");
    }

    #[test]
    fn test_decode_text_latin1_via_encoding_rs() {
        // 0xE9 = é in Latin-1 / Windows-1252
        let bytes = vec![0x48u8, 0x65, 0x6C, 0x6C, 0x6F]; // "Hello"
        let result = decode_text(&bytes, &SasEncoding::Latin1);
        assert_eq!(result, "Hello");

        // é encoded as Latin-1 byte 0xE9
        let bytes = vec![0xE9u8];
        let result = decode_text(&bytes, &SasEncoding::Latin1);
        assert_eq!(result, "é");
    }

    #[test]
    fn test_decode_text_windows1252() {
        // 0x80 = € in Windows-1252 (not valid in Latin-1)
        let bytes = vec![0x80u8];
        let result = decode_text(&bytes, &SasEncoding::Windows1252);
        assert_eq!(result, "€");
    }

    #[test]
    fn test_decode_text_unspecified_defaults_to_windows1252() {
        // Same as Windows-1252 for unspecified
        let bytes = vec![0x80u8];
        let result = decode_text(&bytes, &SasEncoding::Unspecified);
        assert_eq!(result, "€");
    }

    #[test]
    fn test_decode_text_trims_whitespace() {
        let bytes = b"  Hello  ";
        let result = decode_text(bytes, &SasEncoding::Utf8);
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_infer_polars_type_date_formats() {
        for fmt in &[
            "DATE9.",
            "DDMMYY10.",
            "MMDDYY8.",
            "YYMMDD.",
            "JULIAN.",
            "MONYY5.",
            "YYMON.",
            "MONNAME.",
            "WEEKDATE.",
            "WEEKDAY.",
            "QTR.",
            "YEAR.",
            "E8601DA.",
            "B8601DA.",
            "EURDFDD.",
        ] {
            assert_eq!(
                infer_polars_type(fmt, &SasDataType::Numeric),
                PolarsOutputType::Date,
                "Expected Date for format {fmt}"
            );
        }
    }

    #[test]
    fn test_infer_polars_type_datetime_formats() {
        for fmt in &[
            "DATETIME20.",
            "DATETIME.",
            "DTDATE.",
            "DTMONYY.",
            "DTWKDATX.",
            "E8601DT.",
            "B8601DT.",
            "NLDATM.",
            "DATEAMPM.",
        ] {
            assert_eq!(
                infer_polars_type(fmt, &SasDataType::Numeric),
                PolarsOutputType::Datetime,
                "Expected Datetime for format {fmt}"
            );
        }
    }

    #[test]
    fn test_infer_polars_type_dt_prefix_is_datetime() {
        // Any DT-prefixed format should be Datetime
        assert_eq!(
            infer_polars_type("DTCUSTOM.", &SasDataType::Numeric),
            PolarsOutputType::Datetime
        );
    }

    #[test]
    fn test_infer_polars_type_time_formats() {
        for fmt in &[
            "TIME8.",
            "TIME.",
            "TOD.",
            "HHMM.",
            "MMSS.",
            "E8601TM.",
            "B8601TM.",
            "TIMEAMPM.",
            "HOUR.",
        ] {
            assert_eq!(
                infer_polars_type(fmt, &SasDataType::Numeric),
                PolarsOutputType::Time,
                "Expected Time for format {fmt}"
            );
        }
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
        assert_eq!(
            infer_polars_type("monyy5.", &SasDataType::Numeric),
            PolarsOutputType::Date
        );
        assert_eq!(
            infer_polars_type("e8601dt.", &SasDataType::Numeric),
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
