//! Binary format constants for the SAS7BDAT file format.
//!
//! This module defines all magic numbers, offsets, signatures, and sentinel values
//! required to parse SAS7BDAT files according to the binary specification.

// ============================================================================
// T004: Magic Number
// ============================================================================

/// 32-byte magic number at the start of every SAS7BDAT file.
///
/// This signature identifies the file as a valid SAS7BDAT dataset.
/// Location: Offset 0, Length 32 bytes
pub const SAS_MAGIC: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc2, 0xea, 0x81, 0x60,
    0xb3, 0x14, 0x11, 0xcf, 0xbd, 0x92, 0x08, 0x00, 0x09, 0xc7, 0x31, 0x8c, 0x18, 0x1f, 0x10, 0x11,
];

// ============================================================================
// T005: Alignment Detection
// ============================================================================

/// Offset in the header where alignment flag 1 is stored.
/// Determines 64-bit vs 32-bit format (affects page_bit_offset and pointer sizes).
///
/// Location: Offset 32 (0x20)
pub const ALIGN1_FLAG_OFFSET: usize = 32;

/// Offset in the header where alignment flag 2 is stored.
/// Adds 4 to header field offsets when set to 0x33.
///
/// Location: Offset 35 (0x23)
pub const ALIGN2_FLAG_OFFSET: usize = 35;

/// Alignment flag value indicating 32-bit alignment (4-byte pointers).
pub const ALIGN_32BIT: u8 = 0x00;

/// Alignment flag value indicating 64-bit alignment (8-byte pointers).
pub const ALIGN_64BIT: u8 = 0x33;

// ============================================================================
// T006: Endianness Detection
// ============================================================================

/// Offset in the header where endianness flag is stored.
///
/// Location: Offset 37 (0x25)
pub const ENDIAN_FLAG_OFFSET: usize = 37;

/// Endianness flag value for little-endian byte order.
pub const ENDIAN_LITTLE: u8 = 0x01;

/// Endianness flag value for big-endian byte order.
pub const ENDIAN_BIG: u8 = 0x00;

// ============================================================================
// T007: Header Field Offsets
// ============================================================================

/// Offset for text encoding identifier (1 byte).
///
/// Location: Offset 70 (0x46)
pub const ENCODING_OFFSET: usize = 70;

/// Base offset for dataset creation timestamp (double, 8 bytes).
///
/// Actual offset is `TIMESTAMP_CREATED_OFFSET + a1` where `a1` is alignment offset.
pub const TIMESTAMP_CREATED_BASE: usize = 164;

/// Base offset for dataset modification timestamp (double, 8 bytes).
///
/// Actual offset is `TIMESTAMP_MODIFIED_OFFSET + a1` where `a1` is alignment offset.
pub const TIMESTAMP_MODIFIED_BASE: usize = 172;

/// Base offset for page size field (4 bytes, 32-bit int).
///
/// Actual offset is `PAGE_SIZE_OFFSET + a1` where `a1` is alignment offset.
pub const PAGE_SIZE_BASE: usize = 200;

/// Base offset for page count field (4 or 8 bytes depending on alignment).
///
/// Actual offset is `PAGE_COUNT_OFFSET + a1` where `a1` is alignment offset.
pub const PAGE_COUNT_BASE: usize = 204;

/// Base offset for SAS release version string (8 bytes).
/// Actual offset: SAS_RELEASE_BASE + total_align
///
/// Location: Offset 216 (0xD8) for 32-bit with no alignment correction
pub const SAS_RELEASE_BASE: usize = 216;

/// Base offset for SAS server type / platform string (16 bytes).
/// Actual offset: SAS_SERVER_TYPE_BASE + total_align
///
/// Location: Offset 224 (0xE0) for 32-bit with no alignment correction
pub const SAS_SERVER_TYPE_BASE: usize = 224;

/// Base offset for OS version string (16 bytes).
/// Actual offset: OS_VERSION_BASE + total_align
///
/// Location: Offset 240 (0xF0) for 32-bit with no alignment correction
pub const OS_VERSION_BASE: usize = 240;

/// Base offset for OS name string (16 bytes).
/// Actual offset: OS_NAME_BASE + total_align
///
/// Location: Offset 256 (0x100) for 32-bit with no alignment correction
pub const OS_NAME_BASE: usize = 256;

/// Base offset for header length field (4 bytes).
///
/// Actual offset is `HEADER_LENGTH_OFFSET + a1` where `a1` is alignment offset.
pub const HEADER_LENGTH_BASE: usize = 196;

/// Base offset for dataset name in header (64 bytes).
///
/// Actual offset is `DATASET_NAME_OFFSET + a1` where `a1` is alignment offset.
pub const DATASET_NAME_BASE: usize = 92;

/// Alignment offset for 32-bit files (a1 = 0).
pub const ALIGNMENT_OFFSET_32: usize = 0;

/// Alignment offset for 64-bit files (a1 = 4).
pub const ALIGNMENT_OFFSET_64: usize = 4;

// ============================================================================
// T007b: Page Bit Offset Constants
// ============================================================================

/// Number of bytes at the start of each page before the page header fields.
/// For 32-bit files, the page header (type, block_count, subheader_count) starts
/// at offset 16 within each page.
pub const PAGE_BIT_OFFSET_32: usize = 16;

/// Number of bytes at the start of each page before the page header fields.
/// For 64-bit files, the page header starts at offset 32 within each page.
pub const PAGE_BIT_OFFSET_64: usize = 32;

// ============================================================================
// T008: Subheader Signature Byte-Array Constants (LE byte order)
// ============================================================================
//
// These byte-array constants represent subheader signatures as they appear
// in little-endian SAS7BDAT files. They are retained for documentation and
// test use. The integer constants in T008b are used for actual signature
// matching (see subheader.rs).

#[allow(dead_code)]
/// Subheader signature for row size metadata (32-bit).
pub const SUBHEADER_SIGNATURE_ROWSIZE_32: [u8; 4] = [0xF7, 0xF7, 0xF7, 0xF7];

#[allow(dead_code)]
/// Subheader signature for row size metadata (64-bit LE).
pub const SUBHEADER_SIGNATURE_ROWSIZE_64: [u8; 8] =
    [0xF7, 0xF7, 0xF7, 0xF7, 0x00, 0x00, 0x00, 0x00];

#[allow(dead_code)]
/// Subheader signature for column size metadata (32-bit).
pub const SUBHEADER_SIGNATURE_COLUMNSIZE_32: [u8; 4] = [0xF6, 0xF6, 0xF6, 0xF6];

#[allow(dead_code)]
/// Subheader signature for column size metadata (64-bit LE).
pub const SUBHEADER_SIGNATURE_COLUMNSIZE_64: [u8; 8] =
    [0xF6, 0xF6, 0xF6, 0xF6, 0x00, 0x00, 0x00, 0x00];

#[allow(dead_code)]
/// Subheader signature for subheader counts metadata (32-bit).
pub const SUBHEADER_SIGNATURE_COUNTS_32: [u8; 4] = [0x00, 0xFC, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for subheader counts metadata (64-bit LE).
pub const SUBHEADER_SIGNATURE_COUNTS_64: [u8; 8] = [0x00, 0xFC, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for column text metadata (32-bit LE, used in tests).
pub const SUBHEADER_SIGNATURE_COLUMNTEXT_32: [u8; 4] = [0xFD, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for column text metadata (64-bit LE).
pub const SUBHEADER_SIGNATURE_COLUMNTEXT_64: [u8; 8] =
    [0xFD, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for column name metadata (32-bit).
pub const SUBHEADER_SIGNATURE_COLUMNNAME_32: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for column name metadata (64-bit).
pub const SUBHEADER_SIGNATURE_COLUMNNAME_64: [u8; 8] =
    [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for column attributes metadata (32-bit).
pub const SUBHEADER_SIGNATURE_COLUMNATTRS_32: [u8; 4] = [0xFC, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for column attributes metadata (64-bit LE).
pub const SUBHEADER_SIGNATURE_COLUMNATTRS_64: [u8; 8] =
    [0xFC, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for format and label metadata (32-bit).
pub const SUBHEADER_SIGNATURE_FORMAT_32: [u8; 4] = [0xFE, 0xFB, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for format and label metadata (64-bit LE).
pub const SUBHEADER_SIGNATURE_FORMAT_64: [u8; 8] = [0xFE, 0xFB, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for column list metadata (32-bit).
pub const SUBHEADER_SIGNATURE_COLUMNLIST_32: [u8; 4] = [0xFE, 0xFF, 0xFF, 0xFF];

#[allow(dead_code)]
/// Subheader signature for column list metadata (64-bit LE).
pub const SUBHEADER_SIGNATURE_COLUMNLIST_64: [u8; 8] =
    [0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

// ============================================================================
// T008b: Subheader Signature Integer Constants (endianness-safe)
// ============================================================================
//
// Subheader signatures are always 4-byte (u32) values. Reading as integers
// with the file's endianness ensures correct matching for both LE and BE files.
//
// In 64-bit BE files, sign-extended signatures (0xFFFFFFFD etc.) store
// 0xFFFFFFFF in the first 4 bytes with the actual signature in bytes 4-7.
// The subheader matching code handles this by falling back to the second u32
// when the first is 0xFFFFFFFF in 64-bit BE mode.

/// Row size subheader signature (0xF7F7F7F7).
pub const SIG_ROWSIZE_32: u32 = 0xF7F7_F7F7;
/// Column size subheader signature (0xF6F6F6F6).
pub const SIG_COLUMNSIZE_32: u32 = 0xF6F6_F6F6;
/// Column text subheader signature (0xFFFFFFFD).
pub const SIG_COLUMNTEXT_32: u32 = 0xFFFF_FFFD;
/// Column name subheader signature (0xFFFFFFFF).
pub const SIG_COLUMNNAME_32: u32 = 0xFFFF_FFFF;
/// Column attributes subheader signature (0xFFFFFFFC).
pub const SIG_COLUMNATTRS_32: u32 = 0xFFFF_FFFC;
/// Format and label subheader signature (0xFFFFFBFE).
pub const SIG_FORMAT_32: u32 = 0xFFFF_FBFE;

// ============================================================================
// T009: Page Type Constants
// ============================================================================

/// Page type for metadata pages containing column/subheader information.
pub const PAGE_TYPE_META: u16 = 0x0000;

/// Page type for data pages containing observation records.
pub const PAGE_TYPE_DATA: u16 = 0x0100;

/// Page type for mixed pages containing both metadata and data.
pub const PAGE_TYPE_MIX: u16 = 0x0200;

/// Page type for AMD (attribute metadata) pages.
pub const PAGE_TYPE_AMD: u16 = 0x0400;

/// Page type for secondary metadata pages.
pub const PAGE_TYPE_META2: u16 = 0x4000;

/// Page type for compressed data pages.
pub const PAGE_TYPE_COMP: u16 = 0x9000;

// ============================================================================
// T010: Compression Identifier Constants
// ============================================================================

/// Compression signature for RLE (Run-Length Encoding) compression.
///
/// ASCII string "SASYZCRL" indicates RLE-compressed data.
pub const COMPRESSION_SIGNATURE_RLE: [u8; 8] = [b'S', b'A', b'S', b'Y', b'Z', b'C', b'R', b'L'];

/// Compression signature for RDC (Ross Data Compression) compression.
///
/// ASCII string "SASYZCR2" indicates RDC-compressed data.
pub const COMPRESSION_SIGNATURE_RDC: [u8; 8] = [b'S', b'A', b'S', b'Y', b'Z', b'C', b'R', b'2'];

// ============================================================================
// T011: Encoding ID to Name Mapping
// ============================================================================

/// Maps a SAS encoding ID to its canonical character encoding name.
///
/// # Arguments
/// * `id` - The encoding identifier from the SAS7BDAT header (offset 70)
///
/// # Returns
/// * `Some(&str)` with the encoding name if recognized
/// * `None` if the encoding ID is unknown
///
/// # Common Encodings
/// - 20: UTF-8
/// - 28: US-ASCII
/// - 29: ISO-8859-1 (Latin-1)
/// - 62: Windows-1252 (Western European)
/// - 125: EUC-CN (Simplified Chinese)
/// - 134: EUC-JP (Japanese)
/// - 138: Shift_JIS (Japanese)
/// - 140: EUC-KR (Korean)
pub fn encoding_name(id: u16) -> Option<&'static str> {
    match id {
        20 => Some("UTF-8"),
        28 => Some("US-ASCII"),
        29 => Some("ISO-8859-1"),
        33 => Some("ISO-8859-2"),
        34 => Some("ISO-8859-3"),
        35 => Some("ISO-8859-4"),
        36 => Some("ISO-8859-5"),
        37 => Some("ISO-8859-6"),
        38 => Some("ISO-8859-7"),
        39 => Some("ISO-8859-8"),
        40 => Some("ISO-8859-9"),
        60 => Some("Windows-1250"),
        61 => Some("Windows-1251"),
        62 => Some("Windows-1252"),
        63 => Some("Windows-1253"),
        64 => Some("Windows-1254"),
        65 => Some("Windows-1255"),
        66 => Some("Windows-1256"),
        67 => Some("Windows-1257"),
        68 => Some("Windows-1258"),
        123 => Some("Big5"),
        125 => Some("EUC-CN"),
        134 => Some("EUC-JP"),
        138 => Some("Shift_JIS"),
        140 => Some("EUC-KR"),
        _ => None,
    }
}

// ============================================================================
// T012: SAS Epoch Conversion Constants
// ============================================================================

/// Number of days between Unix epoch (1970-01-01) and SAS epoch (1960-01-01).
///
/// SAS dates are stored as days since 1960-01-01, while Unix timestamps use 1970-01-01.
/// This constant converts between the two: `unix_days = sas_days - SAS_EPOCH_OFFSET_DAYS`
pub const SAS_EPOCH_OFFSET_DAYS: i64 = 3653;

/// Number of seconds between Unix epoch (1970-01-01 00:00:00) and SAS epoch (1960-01-01 00:00:00).
///
/// SAS datetimes are stored as seconds since 1960-01-01 00:00:00.
/// This constant converts to Unix timestamps: `unix_seconds = sas_seconds - SAS_EPOCH_OFFSET_SECONDS`
pub const SAS_EPOCH_OFFSET_SECONDS: i64 = 315_619_200;

/// Milliseconds per second for time conversions.
pub const MS_PER_SECOND: i64 = 1000;

/// Nanoseconds per second for time conversions.
pub const NS_PER_SECOND: i64 = 1_000_000_000;

// ============================================================================
// T013: SAS Date Format Patterns
// ============================================================================

/// Common SAS date format prefixes for automatic type detection.
///
/// These patterns identify date-related columns based on their format strings.
pub const DATE_FORMAT_PATTERNS: &[&str] = &[
    "DATE", "DATETIME", "TIME", "DDMMYY", "MMDDYY", "YYMMDD", "YYMMDDD", "JULIAN", "YYMON",
    "MONYY", "MONNAME", "WEEKDATE", "WEEKDAY", "QTR", "YEAR",
];

/// Checks if a SAS format string represents a date/time type.
///
/// # Arguments
/// * `format` - The format string from column metadata (e.g., "DATE9.", "DATETIME20.")
///
/// # Returns
/// * `true` if the format matches a known date/time pattern
/// * `false` otherwise
pub fn is_date_format(format: &str) -> bool {
    let upper = format.to_uppercase();
    DATE_FORMAT_PATTERNS
        .iter()
        .any(|pattern| upper.starts_with(pattern))
}

// ============================================================================
// T014: SAS Missing Value Sentinel Patterns
// ============================================================================

/// First byte value for standard missing value (`.` in SAS).
///
/// In IEEE 754 double representation, SAS uses specific NaN patterns.
/// Standard missing: `0x2E` as first byte in little-endian.
pub const MISSING_STANDARD_BYTE: u8 = 0x2E;

/// First byte values for special missing values (`.A` through `.Z`, `._`).
///
/// SAS supports 27 special missing values encoded with first bytes 0x41-0x5A (A-Z) and 0x5F (_).
pub const MISSING_SPECIAL_BYTES: [u8; 27] = [
    0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, // .A - .I
    0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x50, 0x51, 0x52, // .J - .R
    0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, // .S - .Z
    0x5F, // ._
];

/// Complete 8-byte pattern for standard missing value (`.`) in little-endian.
///
/// This is the full IEEE 754 NaN pattern used by SAS for standard missing.
pub const MISSING_STANDARD_PATTERN_LE: [u8; 8] = [0x2E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0];

/// Complete 8-byte pattern for standard missing value (`.`) in big-endian.
pub const MISSING_STANDARD_PATTERN_BE: [u8; 8] = [0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2E];

/// Checks if an 8-byte double value represents a SAS missing value.
///
/// # Arguments
/// * `bytes` - 8-byte array representing a double in native endianness
/// * `is_little_endian` - Whether the data is little-endian
///
/// # Returns
/// * `true` if the value is a SAS missing value (standard or special)
/// * `false` otherwise
pub fn is_missing_value(bytes: &[u8; 8], is_little_endian: bool) -> bool {
    let first_byte = if is_little_endian { bytes[0] } else { bytes[7] };

    // Check standard missing
    if first_byte == MISSING_STANDARD_BYTE {
        return true;
    }

    // Check special missing (.A - .Z, ._)
    MISSING_SPECIAL_BYTES.contains(&first_byte)
}

/// Total count of SAS missing value types (1 standard + 27 special = 28).
pub const MISSING_VALUE_COUNT: usize = 28;
