//! SAS7BDAT page-level parsing.
//!
//! This module provides functionality to parse page headers and classify
//! page types (metadata, data, mixed, AMD, compressed).
#![allow(dead_code)]

use super::constants::*;
use super::SasError;

/// Page header metadata.
///
/// Each page in a SAS7BDAT file has a header that identifies the page type
/// and the number of subheaders/blocks it contains.
#[derive(Debug, Clone)]
pub struct PageHeader {
    /// Page type identifier (META, DATA, MIX, AMD, META2, COMP).
    pub page_type: u16,
    /// Number of data blocks in this page.
    pub block_count: u16,
    /// Number of subheaders in this page.
    pub subheader_count: u16,
}

/// Parses a page header from page data.
///
/// # Arguments
/// * `page_data` - Raw bytes for the entire page
/// * `is_64bit` - Whether the file uses 64-bit alignment
/// * `is_little_endian` - Whether the file uses little-endian byte order
///
/// # Returns
/// * `Ok(PageHeader)` if parsing succeeded
/// * `Err(SasError)` if page data is malformed
///
/// # Layout
/// Each page starts with a page prefix (page_bit_offset bytes), followed by:
/// - page_type (u16), block_count (u16), subheader_count (u16)
///
/// page_bit_offset is 16 for 32-bit files, 32 for 64-bit files.
pub fn parse_page_header(
    page_data: &[u8],
    is_64bit: bool,
    is_little_endian: bool,
) -> Result<PageHeader, SasError> {
    let page_bit_offset = if is_64bit {
        PAGE_BIT_OFFSET_64
    } else {
        PAGE_BIT_OFFSET_32
    };

    let min_len = page_bit_offset + 6; // type(2) + block_count(2) + subheader_count(2)
    if page_data.len() < min_len {
        return Err(SasError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            format!(
                "Page data too short for page header (need {}, got {})",
                min_len,
                page_data.len()
            ),
        )));
    }

    let o = page_bit_offset;

    let page_type = if is_little_endian {
        u16::from_le_bytes([page_data[o], page_data[o + 1]])
    } else {
        u16::from_be_bytes([page_data[o], page_data[o + 1]])
    };

    let block_count = if is_little_endian {
        u16::from_le_bytes([page_data[o + 2], page_data[o + 3]])
    } else {
        u16::from_be_bytes([page_data[o + 2], page_data[o + 3]])
    };

    let subheader_count = if is_little_endian {
        u16::from_le_bytes([page_data[o + 4], page_data[o + 5]])
    } else {
        u16::from_be_bytes([page_data[o + 4], page_data[o + 5]])
    };

    Ok(PageHeader {
        page_type,
        block_count,
        subheader_count,
    })
}

/// Checks if a page is a metadata page (META or META2).
///
/// Metadata pages contain column definitions and file structure information.
pub fn is_page_meta(page_type: u16) -> bool {
    page_type == PAGE_TYPE_META || page_type == PAGE_TYPE_META2
}

/// Checks if a page is a data-only page.
///
/// Data pages contain only observation records, no metadata.
pub fn is_page_data(page_type: u16) -> bool {
    page_type == PAGE_TYPE_DATA
}

/// Checks if a page is a mixed page (MIX).
///
/// Mixed pages contain both metadata and data records.
pub fn is_page_mix(page_type: u16) -> bool {
    page_type == PAGE_TYPE_MIX
}

/// Checks if a page is an AMD (attribute metadata) page.
///
/// AMD pages contain extended attribute metadata. These are rare and trigger
/// a warning when encountered (transparency principle).
pub fn is_page_amd(page_type: u16) -> bool {
    page_type == PAGE_TYPE_AMD
}

/// Checks if a page is a compressed page (COMP).
///
/// Compressed pages use RLE or RDC compression and require decompression
/// before parsing.
pub fn is_page_comp(page_type: u16) -> bool {
    page_type == PAGE_TYPE_COMP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_page_header_32bit_le() {
        // 32-bit: page header fields start at offset 16 (PAGE_BIT_OFFSET_32)
        let mut page_data = vec![0u8; 24];
        page_data[16] = 0x00;
        page_data[17] = 0x01; // page_type = 0x0100 (DATA)
        page_data[18] = 0x05;
        page_data[19] = 0x00; // block_count = 5
        page_data[20] = 0x03;
        page_data[21] = 0x00; // subheader_count = 3

        let header = parse_page_header(&page_data, false, true).unwrap();
        assert_eq!(header.page_type, 0x0100);
        assert_eq!(header.block_count, 5);
        assert_eq!(header.subheader_count, 3);
    }

    #[test]
    fn test_parse_page_header_32bit_be() {
        // 32-bit: page header fields start at offset 16
        let mut page_data = vec![0u8; 24];
        page_data[16] = 0x01;
        page_data[17] = 0x00; // page_type = 0x0100 (DATA)
        page_data[18] = 0x00;
        page_data[19] = 0x05; // block_count = 5
        page_data[20] = 0x00;
        page_data[21] = 0x03; // subheader_count = 3

        let header = parse_page_header(&page_data, false, false).unwrap();
        assert_eq!(header.page_type, 0x0100);
        assert_eq!(header.block_count, 5);
        assert_eq!(header.subheader_count, 3);
    }

    #[test]
    fn test_parse_page_header_64bit_le() {
        // 64-bit: page header fields start at offset 32 (PAGE_BIT_OFFSET_64)
        let mut page_data = vec![0u8; 40];
        page_data[32] = 0x00;
        page_data[33] = 0x01; // page_type = 0x0100 (DATA)
        page_data[34] = 0x05;
        page_data[35] = 0x00; // block_count = 5
        page_data[36] = 0x03;
        page_data[37] = 0x00; // subheader_count = 3

        let header = parse_page_header(&page_data, true, true).unwrap();
        assert_eq!(header.page_type, 0x0100);
        assert_eq!(header.block_count, 5);
        assert_eq!(header.subheader_count, 3);
    }

    #[test]
    fn test_page_type_classification() {
        assert!(is_page_meta(PAGE_TYPE_META));
        assert!(is_page_meta(PAGE_TYPE_META2));
        assert!(!is_page_meta(PAGE_TYPE_DATA));

        assert!(is_page_data(PAGE_TYPE_DATA));
        assert!(!is_page_data(PAGE_TYPE_META));

        assert!(is_page_mix(PAGE_TYPE_MIX));
        assert!(!is_page_mix(PAGE_TYPE_DATA));

        assert!(is_page_amd(PAGE_TYPE_AMD));
        assert!(!is_page_amd(PAGE_TYPE_META));

        assert!(is_page_comp(PAGE_TYPE_COMP));
        assert!(!is_page_comp(PAGE_TYPE_DATA));
    }

    #[test]
    fn test_parse_page_header_truncated() {
        let page_data = vec![0u8; 20]; // Too short for 32-bit (need 16 + 6 = 22)
        assert!(parse_page_header(&page_data, false, true).is_err());

        let page_data = vec![0u8; 36]; // Too short for 64-bit (need 32 + 6 = 38)
        assert!(parse_page_header(&page_data, true, true).is_err());
    }
}
