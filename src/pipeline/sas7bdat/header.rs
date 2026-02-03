//! SAS7BDAT file header parsing.
//!
//! This module provides functionality to parse the SAS7BDAT file header,
//! extracting critical metadata such as alignment, endianness, encoding,
//! page structure, and dataset dimensions.

use std::io::{Read, Seek, SeekFrom};

use super::constants::*;
use super::{Compression, OsType, SasEncoding, SasError, SasHeader};

/// Parses the SAS7BDAT file header.
///
/// # Arguments
/// * `reader` - A readable and seekable stream positioned at the start of the file
///
/// # Returns
/// * `Ok(SasHeader)` if the header was parsed successfully
/// * `Err(SasError)` if validation or parsing failed
///
/// # Errors
/// * `SasError::InvalidMagic` - File does not start with SAS_MAGIC
/// * `SasError::UnsupportedEncoding` - Unknown encoding ID
/// * `SasError::TruncatedFile` - File size is less than header_length
/// * `SasError::Io` - I/O errors during reading
pub fn parse_header<R: Read + Seek>(reader: &mut R) -> Result<SasHeader, SasError> {
    // Step 1: Validate magic number
    reader.seek(SeekFrom::Start(0))?;
    let mut magic = [0u8; 32];
    reader.read_exact(&mut magic)?;
    if magic != SAS_MAGIC {
        return Err(SasError::InvalidMagic);
    }

    // Step 2: Read alignment flag 1 (offset 32) — determines 64-bit vs 32-bit
    reader.seek(SeekFrom::Start(ALIGN1_FLAG_OFFSET as u64))?;
    let mut align1_byte = [0u8; 1];
    reader.read_exact(&mut align1_byte)?;
    let is_64bit = align1_byte[0] == ALIGN_64BIT;

    // Step 2b: Read alignment flag 2 (offset 35) — additional header offset correction
    reader.seek(SeekFrom::Start(ALIGN2_FLAG_OFFSET as u64))?;
    let mut align2_byte = [0u8; 1];
    reader.read_exact(&mut align2_byte)?;
    let pad1: usize = if align2_byte[0] == ALIGN_64BIT { 4 } else { 0 };

    // Step 3: Read endianness flag (offset 37)
    reader.seek(SeekFrom::Start(ENDIAN_FLAG_OFFSET as u64))?;
    let mut endian_byte = [0u8; 1];
    reader.read_exact(&mut endian_byte)?;
    let is_little_endian = endian_byte[0] == ENDIAN_LITTLE;

    // Step 4: Compute alignment offsets for header field positions
    //
    // The SAS7BDAT header has two independent alignment shifts:
    //   pad1 (from byte 35): shifts fields from offset 164 onwards (timestamps,
    //     header_length, page_size, page_count)
    //   u64_pad (from byte 32): effectively shifts fields AFTER page_count because
    //     page_count is 8 bytes (u64) in 64-bit mode vs 4 bytes (u32) in 32-bit mode
    //
    // Fields before page_count: use pad1 only
    // Fields after page_count (strings): use pad1 + u64_pad = total_align
    let u64_pad: usize = if is_64bit { 4 } else { 0 };
    let total_align = u64_pad + pad1;

    // Step 5: Read encoding (offset 70)
    reader.seek(SeekFrom::Start(ENCODING_OFFSET as u64))?;
    let mut encoding_byte = [0u8; 1];
    reader.read_exact(&mut encoding_byte)?;
    let encoding_id = encoding_byte[0] as u16;
    let encoding = parse_encoding(encoding_id)?;

    // Step 6: Read timestamps (use pad1 only, not total_align)
    let created = read_f64(
        reader,
        (TIMESTAMP_CREATED_BASE + pad1) as u64,
        is_little_endian,
    )?;
    let modified = read_f64(
        reader,
        (TIMESTAMP_MODIFIED_BASE + pad1) as u64,
        is_little_endian,
    )?;

    // Step 7: Read header_length (ALWAYS u32 per readstat reference, use pad1 only)
    let header_length =
        read_u32(reader, (HEADER_LENGTH_BASE + pad1) as u64, is_little_endian)? as u64;

    // Step 8: Read page_size (ALWAYS u32, use pad1 only)
    let page_size = read_u32(reader, (PAGE_SIZE_BASE + pad1) as u64, is_little_endian)?;

    // Step 9: Read page_count (u32 for 32-bit, u64 for 64-bit; use pad1 only)
    let page_count = if is_64bit {
        read_u64(reader, (PAGE_COUNT_BASE + pad1) as u64, is_little_endian)?
    } else {
        read_u32(reader, (PAGE_COUNT_BASE + pad1) as u64, is_little_endian)? as u64
    };

    // Step 10: Read dataset name (before pad1 region, no alignment needed)
    reader.seek(SeekFrom::Start(DATASET_NAME_BASE as u64))?;
    let mut dataset_name_buf = [0u8; 64];
    reader.read_exact(&mut dataset_name_buf)?;
    let dataset_name = parse_null_terminated_string(&dataset_name_buf);

    // Step 11: Read SAS release (AFTER page_count, uses total_align = pad1 + u64_pad)
    reader.seek(SeekFrom::Start((SAS_RELEASE_BASE + total_align) as u64))?;
    let mut sas_release_buf = [0u8; 8];
    reader.read_exact(&mut sas_release_buf)?;
    let sas_release = parse_null_terminated_string(&sas_release_buf);

    // Step 12: Detect OS type from server type and OS name fields (use total_align)
    reader.seek(SeekFrom::Start((SAS_SERVER_TYPE_BASE + total_align) as u64))?;
    let mut server_type_buf = [0u8; 16];
    reader.read_exact(&mut server_type_buf)?;

    reader.seek(SeekFrom::Start((OS_NAME_BASE + total_align) as u64))?;
    let mut os_name_buf = [0u8; 16];
    reader.read_exact(&mut os_name_buf)?;

    // Try server type first (often contains "Linux", "WIN_X64"), fall back to OS name
    let os_type = {
        let server_os = detect_os_type(&server_type_buf);
        if server_os == OsType::Unknown {
            detect_os_type(&os_name_buf)
        } else {
            server_os
        }
    };

    // Step 13: Validate file size
    let file_size = reader.seek(SeekFrom::End(0))?;
    if file_size < header_length {
        return Err(SasError::TruncatedFile {
            expected: header_length,
            actual: file_size,
        });
    }

    // Initialize header with placeholder values for subheader-derived fields
    Ok(SasHeader {
        is_64bit,
        is_little_endian,
        encoding,
        page_size,
        page_count,
        row_count: 0, // Filled from RowSize subheader
        row_length: 0,
        column_count: 0,
        dataset_name,
        created,
        modified,
        header_length,
        compression: Compression::None, // Detected from ColumnText subheader
        os_type,
        sas_release,
        max_rows_on_mix_page: 0, // Filled from RowSize subheader
    })
}

/// Parses encoding ID to SasEncoding enum.
fn parse_encoding(id: u16) -> Result<SasEncoding, SasError> {
    if id == 0 {
        return Ok(SasEncoding::Unspecified);
    }

    match id {
        20 => Ok(SasEncoding::Utf8),
        28 => Ok(SasEncoding::Ascii),
        29 => Ok(SasEncoding::Latin1),
        62 => Ok(SasEncoding::Windows1252),
        _ => {
            if let Some(name) = encoding_name(id) {
                Ok(SasEncoding::Other { id, name })
            } else {
                Err(SasError::UnsupportedEncoding { id })
            }
        }
    }
}

/// Detects OS type from OS name field.
fn detect_os_type(os_name_buf: &[u8]) -> OsType {
    let os_name_str = String::from_utf8_lossy(os_name_buf).to_uppercase();
    if os_name_str.contains("WIN") || os_name_str.contains("W32") {
        OsType::Windows
    } else if os_name_str.contains("UNIX")
        || os_name_str.contains("LINUX")
        || os_name_str.contains("AIX")
        || os_name_str.contains("SUN")
        || os_name_str.contains("HP-UX")
    {
        OsType::Unix
    } else {
        OsType::Unknown
    }
}

/// Parses a null-terminated string from a byte buffer.
fn parse_null_terminated_string(buf: &[u8]) -> String {
    let null_pos = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..null_pos]).trim().to_string()
}

/// Reads a u16 at the specified offset with correct endianness.
#[allow(dead_code)]
fn read_u16<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    is_little_endian: bool,
) -> Result<u16, SasError> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(if is_little_endian {
        u16::from_le_bytes(buf)
    } else {
        u16::from_be_bytes(buf)
    })
}

/// Reads a u32 at the specified offset with correct endianness.
fn read_u32<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    is_little_endian: bool,
) -> Result<u32, SasError> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(if is_little_endian {
        u32::from_le_bytes(buf)
    } else {
        u32::from_be_bytes(buf)
    })
}

/// Reads a u64 at the specified offset with correct endianness.
fn read_u64<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    is_little_endian: bool,
) -> Result<u64, SasError> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(if is_little_endian {
        u64::from_le_bytes(buf)
    } else {
        u64::from_be_bytes(buf)
    })
}

/// Reads a f64 at the specified offset with correct endianness.
fn read_f64<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    is_little_endian: bool,
) -> Result<f64, SasError> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(if is_little_endian {
        f64::from_le_bytes(buf)
    } else {
        f64::from_be_bytes(buf)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_encoding_common() {
        assert!(matches!(parse_encoding(0), Ok(SasEncoding::Unspecified)));
        assert!(matches!(parse_encoding(20), Ok(SasEncoding::Utf8)));
        assert!(matches!(parse_encoding(28), Ok(SasEncoding::Ascii)));
        assert!(matches!(parse_encoding(29), Ok(SasEncoding::Latin1)));
        assert!(matches!(parse_encoding(62), Ok(SasEncoding::Windows1252)));
    }

    #[test]
    fn test_parse_encoding_other() {
        match parse_encoding(125) {
            Ok(SasEncoding::Other { id, name }) => {
                assert_eq!(id, 125);
                assert_eq!(name, "EUC-CN");
            }
            _ => panic!("Expected Other encoding"),
        }
    }

    #[test]
    fn test_parse_encoding_unsupported() {
        assert!(matches!(
            parse_encoding(9999),
            Err(SasError::UnsupportedEncoding { id: 9999 })
        ));
    }

    #[test]
    fn test_detect_os_type() {
        assert_eq!(detect_os_type(b"WIN_X64         "), OsType::Windows);
        assert_eq!(detect_os_type(b"W32_7PRO        "), OsType::Windows);
        assert_eq!(detect_os_type(b"LINUX X64       "), OsType::Unix);
        assert_eq!(detect_os_type(b"UNKNOWN         "), OsType::Unknown);
    }

    #[test]
    fn test_parse_null_terminated_string() {
        assert_eq!(parse_null_terminated_string(b"DATASET\0\0\0"), "DATASET");
        assert_eq!(parse_null_terminated_string(b"DATASET         "), "DATASET");
        assert_eq!(parse_null_terminated_string(b"\0\0\0\0"), "");
    }
}
