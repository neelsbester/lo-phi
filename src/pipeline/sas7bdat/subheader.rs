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

    let start = usize::try_from(pointer.offset).map_err(|_| {
        SasError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Subheader offset {} exceeds platform address space",
                pointer.offset
            ),
        ))
    })?;
    let length = usize::try_from(pointer.length).map_err(|_| {
        SasError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Subheader length {} exceeds platform address space",
                pointer.length
            ),
        ))
    })?;
    let end = start.checked_add(length).ok_or_else(|| {
        SasError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Subheader offset + length overflows",
        ))
    })?;
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
///
/// Entries start at offset `int_len + 8` (signature + metadata header) from the
/// subheader start. The entry count uses the pandas formula:
///   `(subheader_length - 2 * int_len - 12) // 8`
/// This differs from a naive `entries_data.len() / 8` by excluding the trailing
/// `(int_len + 4)` padding bytes that otherwise produce phantom entries.
fn process_columnname_subheader(
    data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    let entry_size = 8; // Same for both 32-bit and 64-bit
    let int_len: usize = if is_64bit { 8 } else { 4 };
    // Entries follow the subheader signature (int_len bytes) + 8-byte metadata header.
    let entries_start = int_len + 8;

    if data.len() < entries_start {
        return Ok(());
    }

    let entries_data = &data[entries_start..];

    // Pandas formula: count = (subheader_length - 2 * int_len - 12) // 8
    // Excludes trailing (int_len + 4) bytes of padding beyond the entry table.
    let num_entries = if data.len() >= 2 * int_len + 12 {
        (data.len() - 2 * int_len - 12) / entry_size
    } else {
        0
    };

    // Additional bounds check against known column count to prevent phantom entries
    let max_entries = if state.column_count_from_size > 0 {
        let remaining =
            state.column_count_from_size as usize - state.column_name_entries.len();
        num_entries.min(remaining)
    } else {
        num_entries
    };

    for i in 0..max_entries {
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
///
/// Entries start at offset `int_len + 8` from the subheader start. Entry count
/// uses the pandas formula: `(subheader_length - 2 * int_len - 12) // (int_len + 8)`
/// This prevents phantom entries from trailing padding beyond the entry table.
/// entry_size is `int_len + 8`: 12 for 32-bit, 16 for 64-bit.
fn process_columnattrs_subheader(
    data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    let int_len: usize = if is_64bit { 8 } else { 4 };
    let entry_size = int_len + 8; // 12 for 32-bit, 16 for 64-bit
    // Entries follow the subheader signature (int_len bytes) + 8-byte metadata header.
    let entries_start = int_len + 8;

    if data.len() < entries_start {
        return Ok(());
    }

    let entries_data = &data[entries_start..];

    // Pandas formula: count = (subheader_length - 2 * int_len - 12) // (int_len + 8)
    // Excludes trailing padding bytes beyond the entry table.
    let num_entries = if data.len() >= 2 * int_len + 12 {
        (data.len() - 2 * int_len - 12) / entry_size
    } else {
        0
    };

    // Bounds-check against known column count to prevent phantom entries
    let max_entries = if state.column_count_from_size > 0 {
        let remaining =
            state.column_count_from_size as usize - state.column_attr_entries.len();
        num_entries.min(remaining)
    } else {
        num_entries
    };

    for i in 0..max_entries {
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
///
/// Each FormatAndLabel subheader describes exactly ONE column — there is no entry
/// table. The SAS runtime emits one subheader invocation per column.
///
/// Field offsets verified against pandas `sas_constants.py` and
/// `_process_format_subheader()`:
///   pandas base formula: `offset + const.FIELD_offset + 3 * int_len`
///   where the constants are: format_text_sub_index=22, format_offset=24,
///   format_length=26, label_text_sub_index=28, label_offset=30, label_length=32.
///
/// Resolved absolute offsets from subheader start:
/// - 32-bit (int_len=4): 34, 36, 38, 40, 42, 44
/// - 64-bit (int_len=8): 46, 48, 50, 52, 54, 56
fn process_format_subheader(
    data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
    state: &mut SubheaderState,
) -> Result<(), SasError> {
    // Minimum subheader length needed to read all six u16 fields.
    let min_len: usize = if is_64bit { 58 } else { 46 };
    if data.len() < min_len {
        return Ok(());
    }

    // Offsets from subheader start: base + 3 * int_len
    // where base constants are 22/24/26/28/30/32 and int_len is 4 (32-bit) or 8 (64-bit).
    let int_len: usize = if is_64bit { 8 } else { 4 };
    let format_text_index_off = 22 + 3 * int_len;
    let format_offset_off = 24 + 3 * int_len;
    let format_length_off = 26 + 3 * int_len;
    let label_text_index_off = 28 + 3 * int_len;
    let label_offset_off = 30 + 3 * int_len;
    let label_length_off = 32 + 3 * int_len;

    let format_text_index = read_u16_from_slice(data, format_text_index_off, is_little_endian);
    let format_offset = read_u16_from_slice(data, format_offset_off, is_little_endian);
    let format_length = read_u16_from_slice(data, format_length_off, is_little_endian);
    let label_text_index = read_u16_from_slice(data, label_text_index_off, is_little_endian);
    let label_offset = read_u16_from_slice(data, label_offset_off, is_little_endian);
    let label_length = read_u16_from_slice(data, label_length_off, is_little_endian);

    state.column_format_entries.push(ColumnFormatEntry {
        format_text_index,
        format_offset,
        format_length,
        label_text_index,
        label_offset,
        label_length,
    });

    Ok(())
}

// Helper functions for reading integers from slices

fn read_u16_from_slice(data: &[u8], offset: usize, is_little_endian: bool) -> u16 {
    if offset + 2 > data.len() {
        return 0;
    }
    if is_little_endian {
        u16::from_le_bytes([data[offset], data[offset + 1]])
    } else {
        u16::from_be_bytes([data[offset], data[offset + 1]])
    }
}

fn read_u32_from_slice(data: &[u8], offset: usize, is_little_endian: bool) -> u32 {
    if offset + 4 > data.len() {
        return 0;
    }
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
    if offset + 8 > data.len() {
        return 0;
    }
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

    /// Verify that process_format_subheader reads a single entry at the correct
    /// fixed offsets (per pandas sas_constants.py formula: base + 3 * int_len).
    /// For 32-bit: offsets 34/36/38/40/42/44. For 64-bit: 46/48/50/52/54/56.
    #[test]
    fn test_format_subheader_32bit_single_entry() {
        let mut state = SubheaderState::default();
        // Minimum size to hold all fields: 46 bytes
        let mut data = vec![0u8; 60];
        // Write known values at 32-bit offsets
        // format_text_index @ 34: 0x0001
        data[34] = 0x01;
        data[35] = 0x00;
        // format_offset @ 36: 0x0010
        data[36] = 0x10;
        data[37] = 0x00;
        // format_length @ 38: 0x0005
        data[38] = 0x05;
        data[39] = 0x00;
        // label_text_index @ 40: 0x0002
        data[40] = 0x02;
        data[41] = 0x00;
        // label_offset @ 42: 0x0020
        data[42] = 0x20;
        data[43] = 0x00;
        // label_length @ 44: 0x0008
        data[44] = 0x08;
        data[45] = 0x00;

        process_format_subheader(&data, false, true, &mut state).unwrap();

        assert_eq!(state.column_format_entries.len(), 1);
        let entry = &state.column_format_entries[0];
        assert_eq!(entry.format_text_index, 1);
        assert_eq!(entry.format_offset, 0x10);
        assert_eq!(entry.format_length, 5);
        assert_eq!(entry.label_text_index, 2);
        assert_eq!(entry.label_offset, 0x20);
        assert_eq!(entry.label_length, 8);
    }

    #[test]
    fn test_format_subheader_64bit_single_entry() {
        let mut state = SubheaderState::default();
        // Minimum size to hold all fields: 58 bytes
        let mut data = vec![0u8; 70];
        // Write known values at 64-bit offsets (46/48/50/52/54/56)
        data[46] = 0x03; // format_text_index
        data[48] = 0x15; // format_offset
        data[50] = 0x07; // format_length
        data[52] = 0x04; // label_text_index
        data[54] = 0x30; // label_offset
        data[56] = 0x0C; // label_length

        process_format_subheader(&data, true, true, &mut state).unwrap();

        assert_eq!(state.column_format_entries.len(), 1);
        let entry = &state.column_format_entries[0];
        assert_eq!(entry.format_text_index, 3);
        assert_eq!(entry.format_offset, 0x15);
        assert_eq!(entry.format_length, 7);
        assert_eq!(entry.label_text_index, 4);
        assert_eq!(entry.label_offset, 0x30);
        assert_eq!(entry.label_length, 0x0C);
    }

    /// A second call must produce a second entry (one per subheader invocation).
    #[test]
    fn test_format_subheader_called_twice_produces_two_entries() {
        let mut state = SubheaderState::default();
        let data = vec![0u8; 60];

        process_format_subheader(&data, false, true, &mut state).unwrap();
        process_format_subheader(&data, false, true, &mut state).unwrap();

        assert_eq!(state.column_format_entries.len(), 2);
    }

    /// Verify column name over-counting fix: pandas formula gives
    /// (subheader_length - 2*int_len - 12) // 8 entries.
    ///
    /// For 32-bit (int_len=4): entries start at offset 12 (4+8), count formula
    /// is (length - 20) // 8. A 28-byte subheader contains 1 valid 8-byte entry
    /// at offset 12 followed by 8 bytes of trailing padding (offsets 20-27).
    /// The old code would count both the entry AND the padding as entries (2 total);
    /// the fixed formula gives (28 - 20) / 8 = 1.
    #[test]
    fn test_columnname_no_phantom_entries_32bit() {
        let mut state = SubheaderState::default();
        // 28 bytes total: 12-byte header prefix + 1 valid 8-byte entry + 8 bytes padding
        // Count formula: (28 - 2*4 - 12) / 8 = (28 - 20) / 8 = 1 entry
        let mut data = vec![0u8; 28];
        // Entry at offset 12 (entries_start = int_len + 8 = 4 + 8 = 12)
        data[12] = 0x01; // text_index lo
        data[13] = 0x00;
        data[14] = 0x05; // name offset lo
        data[15] = 0x00;
        data[16] = 0x04; // name length lo
        data[17] = 0x00;
        // bytes 18-19: remaining entry bytes (unused fields)
        // bytes 20-27: trailing padding (should NOT be parsed as entry)

        process_columnname_subheader(&data, false, true, &mut state).unwrap();

        assert_eq!(state.column_name_entries.len(), 1, "Expected exactly 1 entry");
        assert_eq!(state.column_name_entries[0].text_index, 1);
        assert_eq!(state.column_name_entries[0].offset, 5);
        assert_eq!(state.column_name_entries[0].length, 4);
    }

    /// Verify column attrs over-counting fix for 32-bit: pandas formula gives
    /// (subheader_length - 2*int_len - 12) // (int_len + 8) entries.
    ///
    /// For 32-bit (int_len=4, entry_size=12): entries start at offset 12, count
    /// formula is (length - 20) // 12. A 32-byte subheader contains 1 valid
    /// 12-byte entry at offset 12 followed by 8 bytes of trailing padding.
    /// The old code counted (32 - 12) / 12 = 1 entry (coincidentally correct here),
    /// but for subheaders with different padding the count can differ.
    #[test]
    fn test_columnattrs_no_phantom_entries_32bit() {
        let mut state = SubheaderState::default();
        // 32 bytes total: 12-byte prefix + 1 valid 12-byte entry + 8 bytes padding
        // Count formula: (32 - 2*4 - 12) / 12 = (32 - 20) / 12 = 1 entry
        let mut data = vec![0u8; 32];
        // Entry at offset 12: col_offset=0x0001, width=8, type=1 (Numeric)
        data[12] = 0x01; // col offset lo byte
        data[13] = 0x00;
        data[14] = 0x00;
        data[15] = 0x00;
        data[16] = 0x08; // width lo byte (u32)
        data[17] = 0x00;
        data[18] = 0x00;
        data[19] = 0x00;
        // bytes 20-21: unknown
        // byte 22: type = 1 (Numeric) — offset within entry: 22 - 12 = 10
        data[22] = 0x01;
        // byte 23: padding

        process_columnattrs_subheader(&data, false, true, &mut state).unwrap();

        assert_eq!(state.column_attr_entries.len(), 1, "Expected exactly 1 entry");
        assert_eq!(state.column_attr_entries[0].offset, 1);
        assert_eq!(state.column_attr_entries[0].length, 8);
        assert!(matches!(
            state.column_attr_entries[0].data_type,
            SasDataType::Numeric
        ));
    }

    /// Verify bounds check against column_count_from_size limits extra entries.
    /// For 32-bit: entries start at offset 12, count formula (36-20)/8 = 2.
    /// With column_count_from_size=1, only the first entry should be returned.
    #[test]
    fn test_columnname_capped_by_column_count() {
        let mut state = SubheaderState {
            column_count_from_size: 1, // Only 1 column expected
            ..SubheaderState::default()
        };
        // 36 bytes: 12 prefix + 2 valid 8-byte entries (count formula: (36-20)/8=2)
        let mut data = vec![0u8; 36];
        data[12] = 0x01; // first entry text_index at offset 12
        data[20] = 0x02; // second entry text_index at offset 20

        process_columnname_subheader(&data, false, true, &mut state).unwrap();

        assert_eq!(
            state.column_name_entries.len(),
            1,
            "Should be capped at column_count_from_size=1"
        );
    }
}
