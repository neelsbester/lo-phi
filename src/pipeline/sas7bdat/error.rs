//! Error types for SAS7BDAT file parsing.
//!
//! This module defines the `SasError` enum and related error handling for
//! parsing SAS7BDAT files. Each variant captures specific failure modes
//! during file validation, decompression, and data reconstruction.

use std::fmt;

/// Errors that can occur when parsing SAS7BDAT files.
#[derive(Debug)]
pub enum SasError {
    /// File does not start with the SAS7BDAT magic number.
    ///
    /// The SAS7BDAT format requires specific magic bytes at the start of the file.
    /// This error indicates the file is not a valid SAS7BDAT file.
    InvalidMagic,

    /// File is truncated (shorter than expected based on header metadata).
    ///
    /// The header indicates the file should be `expected` bytes long, but only
    /// `actual` bytes were found. This typically indicates file corruption or
    /// incomplete download.
    TruncatedFile {
        /// Expected file size in bytes (from header)
        expected: u64,
        /// Actual file size in bytes
        actual: u64,
    },

    /// File contains zero data rows.
    ///
    /// The SAS7BDAT file has valid structure but no data rows. While technically
    /// valid, this is often unexpected and flagged as an error.
    ZeroRows,

    /// Character encoding is not supported.
    ///
    /// SAS7BDAT files can use various character encodings. This error indicates
    /// the encoding ID found in the file is not recognized or supported.
    UnsupportedEncoding {
        /// Encoding identifier from file header
        id: u16,
    },

    /// Page type is not recognized.
    ///
    /// Each page in a SAS7BDAT file has a type indicator. This error occurs when
    /// an unknown page type is encountered.
    InvalidPageType {
        /// Zero-based index of the page in the file
        page_index: u64,
        /// Page type value that was not recognized
        page_type: u16,
    },

    /// Subheader signature is not recognized.
    ///
    /// Subheaders identify different metadata sections. This error occurs when
    /// a subheader with an unknown signature is encountered.
    UnknownSubheader {
        /// Subheader signature bytes
        signature: Vec<u8>,
        /// Byte offset where the subheader was found
        offset: u64,
    },

    /// Decompression failed for a compressed page.
    ///
    /// SAS7BDAT files can use RLE or RDC compression. This error occurs when
    /// decompression fails.
    DecompressionError {
        /// Zero-based index of the page that failed to decompress
        page_index: u64,
        /// Detailed error message from decompression
        message: String,
    },

    /// Numeric value reconstruction failed.
    ///
    /// This error occurs when converting raw bytes to a numeric value fails,
    /// typically due to invalid IEEE 754 encoding or unexpected bit patterns.
    NumericError {
        /// Column name where the error occurred
        column: String,
        /// Zero-based row index where the error occurred
        row: u64,
        /// Detailed error message
        message: String,
    },

    /// I/O error occurred while reading the file.
    ///
    /// This wraps standard I/O errors (e.g., file not found, permission denied,
    /// read failures).
    Io(std::io::Error),
}

impl fmt::Display for SasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SasError::InvalidMagic => {
                write!(f, "Invalid SAS7BDAT file: magic number mismatch")
            }
            SasError::TruncatedFile { expected, actual } => {
                write!(
                    f,
                    "Truncated SAS7BDAT file: expected {} bytes, found {}",
                    expected, actual
                )
            }
            SasError::ZeroRows => {
                write!(f, "SAS7BDAT file contains zero data rows")
            }
            SasError::UnsupportedEncoding { id } => {
                write!(
                    f,
                    "Unsupported character encoding in SAS7BDAT file: encoding ID {}",
                    id
                )
            }
            SasError::InvalidPageType {
                page_index,
                page_type,
            } => {
                write!(
                    f,
                    "Invalid page type {} at page index {}",
                    page_type, page_index
                )
            }
            SasError::UnknownSubheader { signature, offset } => {
                let sig_hex = signature
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                write!(
                    f,
                    "Unknown subheader signature [{}] at byte offset {}",
                    sig_hex, offset
                )
            }
            SasError::DecompressionError {
                page_index,
                message,
            } => {
                write!(
                    f,
                    "Decompression failed for page {}: {}",
                    page_index, message
                )
            }
            SasError::NumericError {
                column,
                row,
                message,
            } => {
                write!(
                    f,
                    "Numeric error in column '{}' at row {}: {}",
                    column, row, message
                )
            }
            SasError::Io(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl std::error::Error for SasError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SasError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SasError {
    fn from(err: std::io::Error) -> Self {
        SasError::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::io;

    #[test]
    fn test_invalid_magic_display() {
        let err = SasError::InvalidMagic;
        assert_eq!(
            err.to_string(),
            "Invalid SAS7BDAT file: magic number mismatch"
        );
    }

    #[test]
    fn test_truncated_file_display() {
        let err = SasError::TruncatedFile {
            expected: 10000,
            actual: 5000,
        };
        assert_eq!(
            err.to_string(),
            "Truncated SAS7BDAT file: expected 10000 bytes, found 5000"
        );
    }

    #[test]
    fn test_zero_rows_display() {
        let err = SasError::ZeroRows;
        assert_eq!(err.to_string(), "SAS7BDAT file contains zero data rows");
    }

    #[test]
    fn test_unsupported_encoding_display() {
        let err = SasError::UnsupportedEncoding { id: 999 };
        assert_eq!(
            err.to_string(),
            "Unsupported character encoding in SAS7BDAT file: encoding ID 999"
        );
    }

    #[test]
    fn test_invalid_page_type_display() {
        let err = SasError::InvalidPageType {
            page_index: 42,
            page_type: 128,
        };
        assert_eq!(err.to_string(), "Invalid page type 128 at page index 42");
    }

    #[test]
    fn test_unknown_subheader_display() {
        let err = SasError::UnknownSubheader {
            signature: vec![0xDE, 0xAD, 0xBE, 0xEF],
            offset: 1024,
        };
        assert_eq!(
            err.to_string(),
            "Unknown subheader signature [de ad be ef] at byte offset 1024"
        );
    }

    #[test]
    fn test_decompression_error_display() {
        let err = SasError::DecompressionError {
            page_index: 10,
            message: "Invalid RLE sequence".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Decompression failed for page 10: Invalid RLE sequence"
        );
    }

    #[test]
    fn test_numeric_error_display() {
        let err = SasError::NumericError {
            column: "age".to_string(),
            row: 123,
            message: "Invalid IEEE 754 encoding".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Numeric error in column 'age' at row 123: Invalid IEEE 754 encoding"
        );
    }

    #[test]
    fn test_io_error_display() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err = SasError::Io(io_err);
        assert!(err.to_string().contains("I/O error"));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_io_error_source() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let err = SasError::Io(io_err);
        assert!(err.source().is_some());
    }

    #[test]
    fn test_non_io_error_source() {
        let err = SasError::InvalidMagic;
        assert!(err.source().is_none());
    }

    #[test]
    fn test_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected EOF");
        let sas_err: SasError = io_err.into();
        assert!(matches!(sas_err, SasError::Io(_)));
        assert!(sas_err.to_string().contains("unexpected EOF"));
    }
}
