//! SAS7BDAT subheader parsing.
//!
//! This module provides functionality to parse subheader pointers and
//! extract metadata from various subheader types (RowSize, ColumnSize,
//! ColumnText, ColumnName, ColumnAttributes, FormatAndLabel).

use super::constants::*;
use super::{Compression, SasDataType, SasError};

/// A subheader pointer within a page.
///
/// Subheader pointers form a table at the start of metadata pages,
/// directing the parser to specific subheader locations within the page.
#[derive(Debug, Clone)]
pub struct SubheaderPointer {
    /// Byte offset of the subheader within the page.
    pub offset: u64,
    /// Length of the subheader in bytes.
    pub length: u64,
    /// Compression indicator (0 = uncompressed, 1 = truncated, 4 = compressed).
    pub compression: u8,
    /// Subheader type indicator.
    pub subheader_type: u8,
}

/// Accumulated state from parsing all subheaders.
///
/// As subheaders are processed, this structure accumulates metadata
/// that will be used to construct the final column list and populate
/// the SasHeader.
#[derive(Debug, Clone)]
pub struct SubheaderState {
    /// Length of each row in bytes (from RowSize subheader).
    pub row_length: u64,
    /// Total number of rows in the dataset (from RowSize subheader).
    pub row_count: u64,
    /// Number of columns (from ColumnSize subheader).
    pub column_count_from_size: u64,
    /// Maximum rows that can fit on a MIX page (from RowSize subheader).
    pub max_rows_on_mix_page: u64,
    /// Text blocks containing column names, formats, and labels.
    pub column_text_blocks: Vec<Vec<u8>>,
    /// Column name metadata (text_index, offset, length).
    pub column_name_entries: Vec<ColumnNameEntry>,
    /// Column attribute metadata (offset, length, data_type).
    pub column_attr_entries: Vec<ColumnAttrEntry>,
    /// Column format and label metadata.
    pub column_format_entries: Vec<ColumnFormatEntry>,
    /// Detected compression method (from ColumnText signature).
    pub compression: Compression,
}

impl Default for SubheaderState {
    fn default() -> Self {
        Self {
            row_length: 0,
            row_count: 0,
            column_count_from_size: 0,
            max_rows_on_mix_page: 0,
            column_text_blocks: Vec::new(),
            column_name_entries: Vec::new(),
            column_attr_entries: Vec::new(),
            column_format_entries: Vec::new(),
            compression: Compression::None,
        }
    }
}

/// Column name entry from ColumnName subheader.
#[derive(Debug, Clone)]
pub struct ColumnNameEntry {
    /// Index into column_text_blocks.
    pub text_index: u16,
    /// Byte offset within the text block.
    pub offset: u16,
    /// Length of the name in bytes.
    pub length: u16,
}

/// Column attribute entry from ColumnAttributes subheader.
#[derive(Debug, Clone)]
pub struct ColumnAttrEntry {
    /// Byte offset of this column within each row.
    pub offset: u64,
    /// Length of the column in bytes.
    pub length: u32,
    /// Native SAS data type (Numeric or Character).
    pub data_type: SasDataType,
}

/// Column format and label entry from FormatAndLabel subheader.
#[derive(Debug, Clone, Default)]
pub struct ColumnFormatEntry {
    /// Index into column_text_blocks for format string.
    pub format_text_index: u16,
    /// Byte offset within the text block for format.
    pub format_offset: u16,
    /// Length of the format string in bytes.
    pub format_length: u16,
    /// Index into column_text_blocks for label string.
    pub label_text_index: u16,
    /// Byte offset within the text block for label.
    pub label_offset: u16,
    /// Length of the label string in bytes.
    pub label_length: u16,
}

/// Parses subheader pointers from a page.
///
/// # Arguments
/// * `page_data` - Raw bytes for the entire page
/// * `is_64bit` - Whether the file uses 64-bit alignment
/// * `is_little_endian` - Whether the file uses little-endian byte order
/// * `subheader_count` - Number of subheader pointers to parse
///
/// # Returns
/// * `Ok(Vec<SubheaderPointer>)` if parsing succeeded
/// * `Err(SasError)` if page data is malformed
///
/// # Layout
/// Pointer table starts immediately after page header:
/// - 32-bit: Each pointer is 12 bytes (offset u32, length u32, compression u8, type u8)
/// - 64-bit: Each pointer is 24 bytes (offset u64, length u64, compression u8, type u8)
pub fn parse_subheader_pointers(
    page_data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    subheader_count: u16,
) -> Result<Vec<SubheaderPointer>, SasError> {
    // Pointer table starts after page prefix (page_bit_offset) + page header (8 bytes)
    let page_bit_offset = if is_64bit { 32 } else { 16 };
    let pointer_start = page_bit_offset + 8; // 24 for 32-bit, 40 for 64-bit
    let pointer_size = if is_64bit { 24 } else { 12 };
    let mut pointers = Vec::new();

    for i in 0..subheader_count {
        let offset_in_page = pointer_start + (i as usize * pointer_size);
        if offset_in_page + pointer_size > page_data.len() {
            return Err(SasError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Subheader pointer table exceeds page bounds",
            )));
        }

        let ptr_data = &page_data[offset_in_page..offset_in_page + pointer_size];

        let (offset, length, compression, subheader_type) = if is_64bit {
            let offset = read_u64_from_slice(ptr_data, 0, is_little_endian);
            let length = read_u64_from_slice(ptr_data, 8, is_little_endian);
            let compression = ptr_data[16];
            let subheader_type = ptr_data[17];
            (offset, length, compression, subheader_type)
        } else {
            let offset = read_u32_from_slice(ptr_data, 0, is_little_endian) as u64;
            let length = read_u32_from_slice(ptr_data, 4, is_little_endian) as u64;
            let compression = ptr_data[8];
            let subheader_type = ptr_data[9];
            (offset, length, compression, subheader_type)
        };

        pointers.push(SubheaderPointer {
            offset,
            length,
            compression,
            subheader_type,
        });
    }

    Ok(pointers)
}

/// Processes a single subheader and updates state.
///
/// # Arguments
/// * `page_data` - Raw bytes for the entire page
/// * `pointer` - Subheader pointer metadata
/// * `is_64bit` - Whether the file uses 64-bit alignment
/// * `is_little_endian` - Whether the file uses little-endian byte order
/// * `state` - Accumulated subheader state to update
///
/// # Returns
/// * `Ok(())` if processing succeeded
/// * `Err(SasError)` if subheader is malformed or unrecognized
pub fn process_subheader(
    page_data: &[u8],
    pointer: &SubheaderPointer,
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    if pointer.length == 0 {
        return Ok(()); // Empty subheader, skip
    }

    let start = pointer.offset as usize;
    let end = start + pointer.length as usize;
    if end > page_data.len() {
        return Err(SasError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "Subheader exceeds page bounds",
        )));
    }

    let subheader_data = &page_data[start..end];

    // Subheader signatures are fundamentally 4 bytes (u32). Read with the
    // file's endianness to handle byte-swapped signatures in BE files.
    //
    // In 64-bit BE files, some signatures have their distinguishing byte in
    // the second u32 (bytes 4-7) because the BE representation of the
    // sign-extended 64-bit value puts 0xFFFFFFFF in the first 4 bytes.
    // For example, COLUMNTEXT (0xFFFFFFFD) in 64-bit BE is stored as
    // [FF FF FF FF FF FF FF FD] - the first u32 is 0xFFFFFFFF (ambiguous).
    if subheader_data.len() < 4 {
        return Ok(()); // Too short, skip
    }

    let mut sig = read_u32_from_slice(subheader_data, 0, is_little_endian);

    // In 64-bit BE mode, when first u32 is 0xFFFFFFFF, the actual signature
    // is in bytes 4-7 (the low word of the sign-extended 64-bit value).
    if is_64bit && !is_little_endian && sig == 0xFFFFFFFF && subheader_data.len() >= 8 {
        sig = read_u32_from_slice(subheader_data, 4, is_little_endian);
    }

    match sig {
        SIG_ROWSIZE_32 => {
            process_rowsize_subheader(subheader_data, is_64bit, is_little_endian, state)?;
        }
        SIG_COLUMNSIZE_32 => {
            process_columnsize_subheader(subheader_data, is_64bit, is_little_endian, state)?;
        }
        SIG_COLUMNTEXT_32 => {
            process_columntext_subheader(subheader_data, is_64bit, state)?;
        }
        SIG_COLUMNNAME_32 => {
            process_columnname_subheader(subheader_data, is_64bit, is_little_endian, state)?;
        }
        SIG_COLUMNATTRS_32 => {
            process_columnattrs_subheader(subheader_data, is_64bit, is_little_endian, state)?;
        }
        SIG_FORMAT_32 => {
            process_format_subheader(subheader_data, is_64bit, is_little_endian, state)?;
        }
        _ => {} // Ignore COUNTS, COLUMNLIST, and unknown subheaders
    }

    Ok(())
}

/// Processes RowSize subheader.
fn process_rowsize_subheader(
    data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    if is_64bit {
        // 64-bit layout: row_length @ 40, row_count @ 48, max_mix_page_rows @ 120
        if data.len() >= 56 {
            state.row_length = read_u64_from_slice(data, 40, is_little_endian);
            state.row_count = read_u64_from_slice(data, 48, is_little_endian);
        }
        if data.len() >= 128 {
            state.max_rows_on_mix_page = read_u64_from_slice(data, 120, is_little_endian);
        }
    } else {
        // 32-bit layout: row_length @ 20, row_count @ 24, max_mix_page_rows @ 60
        if data.len() >= 28 {
            state.row_length = read_u32_from_slice(data, 20, is_little_endian) as u64;
            state.row_count = read_u32_from_slice(data, 24, is_little_endian) as u64;
        }
        if data.len() >= 64 {
            state.max_rows_on_mix_page = read_u32_from_slice(data, 60, is_little_endian) as u64;
        }
    }
    Ok(())
}

/// Processes ColumnSize subheader.
fn process_columnsize_subheader(
    data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    // Column count location varies; try common offsets
    if is_64bit {
        if data.len() >= 16 {
            state.column_count_from_size = read_u64_from_slice(data, 8, is_little_endian);
        }
    } else if data.len() >= 8 {
        state.column_count_from_size = read_u32_from_slice(data, 4, is_little_endian) as u64;
    }
    Ok(())
}

/// Processes ColumnText subheader (contains text blocks and compression signature).
///
/// The text block is stored AFTER the subheader signature (4 bytes for 32-bit,
/// 8 bytes for 64-bit). Column name and format offsets reference positions
/// within this stored text block (i.e., relative to after the signature).
fn process_columntext_subheader(
    data: &[u8],
    is_64bit: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    let sig_len = if is_64bit { 8 } else { 4 };

    if sig_len >= data.len() {
        return Ok(());
    }

    let text_block = &data[sig_len..];

    // Check for compression signature in the first text block.
    // The compression literal ("SASYZCRL" or "SASYZCR2") is always at offset 12
    // within the text_block (i.e., after the subheader signature bytes).
    // This is offset 16 from subheader start for 32-bit (sig=4, 4+12=16) and
    // offset 20 from subheader start for 64-bit (sig=8, 8+12=20).
    if state.column_text_blocks.is_empty() {
        let comp_offset: usize = 12;
        if text_block.len() >= comp_offset + 8 {
            let sig_check = &text_block[comp_offset..comp_offset + 8];
            if sig_check == COMPRESSION_SIGNATURE_RLE {
                state.compression = Compression::Rle;
            } else if sig_check == COMPRESSION_SIGNATURE_RDC {
                state.compression = Compression::Rdc;
            }
        }
    }

    state.column_text_blocks.push(text_block.to_vec());

    Ok(())
}

/// Processes ColumnName subheader (contains name entries).
fn process_columnname_subheader(
    data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    let entry_size = 8; // Same for both 32-bit and 64-bit
                        // Entries start after signature (4/8 bytes) + 8 bytes of metadata
    let entries_start = if is_64bit { 16 } else { 12 };

    if data.len() < entries_start {
        return Ok(());
    }

    let entries_data = &data[entries_start..];
    let num_entries = entries_data.len() / entry_size;

    for i in 0..num_entries {
        let entry_offset = i * entry_size;
        if entry_offset + entry_size > entries_data.len() {
            break;
        }

        let entry = &entries_data[entry_offset..entry_offset + entry_size];
        let text_index = read_u16_from_slice(entry, 0, is_little_endian);
        let offset = read_u16_from_slice(entry, 2, is_little_endian);
        let length = read_u16_from_slice(entry, 4, is_little_endian);

        state.column_name_entries.push(ColumnNameEntry {
            text_index,
            offset,
            length,
        });
    }

    Ok(())
}

/// Processes ColumnAttributes subheader (contains type, offset, length).
fn process_columnattrs_subheader(
    data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    let entry_size = if is_64bit { 16 } else { 12 };
    let entries_start = if is_64bit { 16 } else { 12 };

    if data.len() < entries_start {
        return Ok(());
    }

    let entries_data = &data[entries_start..];
    let num_entries = entries_data.len() / entry_size;

    for i in 0..num_entries {
        let entry_offset = i * entry_size;
        if entry_offset + entry_size > entries_data.len() {
            break;
        }

        let entry = &entries_data[entry_offset..entry_offset + entry_size];

        // Entry layout:
        // 32-bit (12 bytes): offset(u32@0), width(u32@4), unk(@8), unk(@9), type(@10), pad(@11)
        // 64-bit (16 bytes): offset(u64@0), width(u32@8), unk(@12), unk(@13), type(@14), pad(@15)
        let (offset, length, data_type_byte) = if is_64bit {
            let offset = read_u64_from_slice(entry, 0, is_little_endian);
            let length = read_u32_from_slice(entry, 8, is_little_endian);
            let data_type_byte = entry[14];
            (offset, length, data_type_byte)
        } else {
            let offset = read_u32_from_slice(entry, 0, is_little_endian) as u64;
            let length = read_u32_from_slice(entry, 4, is_little_endian);
            let data_type_byte = entry[10];
            (offset, length, data_type_byte)
        };

        let data_type = if data_type_byte == 1 {
            SasDataType::Numeric
        } else {
            SasDataType::Character
        };

        state.column_attr_entries.push(ColumnAttrEntry {
            offset,
            length,
            data_type,
        });
    }

    Ok(())
}

/// Processes FormatAndLabel subheader (contains format and label text references).
fn process_format_subheader(
    data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    let entry_size = if is_64bit { 52 } else { 46 };
    let entries_start = if is_64bit { 16 } else { 12 };

    if data.len() < entries_start {
        return Ok(());
    }

    let entries_data = &data[entries_start..];
    let num_entries = entries_data.len() / entry_size;

    for i in 0..num_entries {
        let entry_offset = i * entry_size;
        if entry_offset + entry_size > entries_data.len() {
            break;
        }

        let entry = &entries_data[entry_offset..entry_offset + entry_size];

        let format_text_index = read_u16_from_slice(entry, 0, is_little_endian);
        let format_offset = read_u16_from_slice(entry, 2, is_little_endian);
        let format_length = read_u16_from_slice(entry, 4, is_little_endian);
        let label_text_index = read_u16_from_slice(entry, 6, is_little_endian);
        let label_offset = read_u16_from_slice(entry, 8, is_little_endian);
        let label_length = read_u16_from_slice(entry, 10, is_little_endian);

        state.column_format_entries.push(ColumnFormatEntry {
            format_text_index,
            format_offset,
            format_length,
            label_text_index,
            label_offset,
            label_length,
        });
    }

    Ok(())
}

// Helper functions for reading integers from slices

fn read_u16_from_slice(data: &[u8], offset: usize, is_little_endian: bool) -> u16 {
    if is_little_endian {
        u16::from_le_bytes([data[offset], data[offset + 1]])
    } else {
        u16::from_be_bytes([data[offset], data[offset + 1]])
    }
}

fn read_u32_from_slice(data: &[u8], offset: usize, is_little_endian: bool) -> u32 {
    if is_little_endian {
        u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ])
    } else {
        u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ])
    }
}

fn read_u64_from_slice(data: &[u8], offset: usize, is_little_endian: bool) -> u64 {
    if is_little_endian {
        u64::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ])
    } else {
        u64::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_subheader_pointers_32bit() {
        let mut page_data = vec![0u8; 100];
        // Pointer table starts at page_bit_offset(16) + 8 = 24 for 32-bit
        // Pointer 1 at offset 24
        page_data[24] = 0x20; // offset = 0x20
        page_data[28] = 0x10; // length = 0x10
        page_data[32] = 0x00; // compression
        page_data[33] = 0x01; // type

        let pointers = parse_subheader_pointers(&page_data, false, true, 1).unwrap();
        assert_eq!(pointers.len(), 1);
        assert_eq!(pointers[0].offset, 0x20);
        assert_eq!(pointers[0].length, 0x10);
        assert_eq!(pointers[0].compression, 0x00);
        assert_eq!(pointers[0].subheader_type, 0x01);
    }

    #[test]
    fn test_subheader_state_default() {
        let state = SubheaderState::default();
        assert_eq!(state.row_length, 0);
        assert_eq!(state.row_count, 0);
        assert_eq!(state.column_count_from_size, 0);
        assert!(matches!(state.compression, Compression::None));
    }

    #[test]
    fn test_compression_detection() {
        let mut state = SubheaderState::default();
        // For 32-bit: 4-byte signature, then text block starts at offset 4.
        // Compression signature is at fixed offset 16 from subheader start,
        // which is offset 12 within the text block.
        let mut data = vec![0u8; 24];
        data[0..4].copy_from_slice(&SUBHEADER_SIGNATURE_COLUMNTEXT_32);
        // Compression signature at offset 16 in full data = offset 12 in text block
        data[16..24].copy_from_slice(&COMPRESSION_SIGNATURE_RLE);

        process_columntext_subheader(&data, false, &mut state).unwrap();
        assert!(matches!(state.compression, Compression::Rle));
    }

    #[test]
    fn test_compression_detection_64bit_rle() {
        let mut state = SubheaderState::default();
        // For 64-bit: 8-byte signature, then text block starts at offset 8.
        // Compression signature is at fixed offset 20 from subheader start,
        // which is offset 12 within the text block (same as 32-bit).
        let mut data = vec![0u8; 28];
        data[0..8].copy_from_slice(&SUBHEADER_SIGNATURE_COLUMNTEXT_64);
        // Compression signature at offset 20 in full data = offset 12 in text block
        data[20..28].copy_from_slice(&COMPRESSION_SIGNATURE_RLE);

        process_columntext_subheader(&data, true, &mut state).unwrap();
        assert!(
            matches!(state.compression, Compression::Rle),
            "64-bit RLE compression should be detected, got {:?}",
            state.compression
        );
    }

    #[test]
    fn test_compression_detection_64bit_rdc() {
        let mut state = SubheaderState::default();
        // Same layout for RDC compression in 64-bit files.
        let mut data = vec![0u8; 28];
        data[0..8].copy_from_slice(&SUBHEADER_SIGNATURE_COLUMNTEXT_64);
        data[20..28].copy_from_slice(&COMPRESSION_SIGNATURE_RDC);

        process_columntext_subheader(&data, true, &mut state).unwrap();
        assert!(
            matches!(state.compression, Compression::Rdc),
            "64-bit RDC compression should be detected, got {:?}",
            state.compression
        );
    }

    #[test]
    fn test_read_helpers() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];

        assert_eq!(read_u16_from_slice(&data, 0, true), 0x3412);
        assert_eq!(read_u16_from_slice(&data, 0, false), 0x1234);

        assert_eq!(read_u32_from_slice(&data, 0, true), 0x78563412);
        assert_eq!(read_u32_from_slice(&data, 0, false), 0x12345678);

        assert_eq!(read_u64_from_slice(&data, 0, true), 0xF0DEBC9A78563412);
        assert_eq!(read_u64_from_slice(&data, 0, false), 0x123456789ABCDEF0);
    }
}
