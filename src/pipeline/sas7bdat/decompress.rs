//! Decompression algorithms for SAS7BDAT compressed pages.
//!
//! SAS7BDAT files support two compression methods:
//! - **RLE (Run-Length Encoding)**: Command-based compression with literal copies and run-length sequences
//! - **RDC (Ross Data Compression)**: LZ77-style compression with back-references and run-length encoding
//!
//! Both algorithms decompress page data to the expected output length specified in page headers.

use crate::pipeline::sas7bdat::error::SasError;

/// Decompress RLE-compressed data.
///
/// RLE uses a control-byte format where each control byte encodes a command and length:
/// - Command = upper 4 bits: `(control >> 4) & 0x0F`
/// - Length = lower 4 bits: `control & 0x0F`
///
/// # Arguments
///
/// * `input` - Compressed byte stream
/// * `output_length` - Expected decompressed size in bytes
///
/// # Returns
///
/// Decompressed byte vector of exactly `output_length` bytes.
///
/// # Errors
///
/// Returns `SasError::DecompressionError` if:
/// - Input is exhausted prematurely
/// - Output buffer overflow occurs
/// - Unknown command code is encountered
///
/// # Reference
///
/// Based on ReadStat RLE decompression algorithm.
pub fn decompress_rle(input: &[u8], output_length: usize) -> Result<Vec<u8>, SasError> {
    let mut output = Vec::with_capacity(output_length);
    let mut input_pos = 0;

    while output.len() < output_length {
        // Read control byte
        if input_pos >= input.len() {
            return Err(SasError::DecompressionError {
                page_index: 0,
                message: format!(
                    "Premature end of input at position {} (output {} of {})",
                    input_pos,
                    output.len(),
                    output_length
                ),
            });
        }

        let control = input[input_pos];
        input_pos += 1;

        let command = (control >> 4) & 0x0F;
        let length = (control & 0x0F) as usize;

        match command {
            // COPY64: Copy next_byte + 64 + length*256 bytes
            0x0 => {
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!("COPY64: missing next_byte at position {}", input_pos),
                    });
                }
                let next_byte = input[input_pos] as usize;
                input_pos += 1;
                let count = next_byte + 64 + length * 256;
                copy_bytes(input, &mut input_pos, &mut output, count, output_length)?;
            }

            // COPY64_PLUS_4096: Copy next_byte + 64 + length*256 + 4096 bytes
            0x1 => {
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "COPY64_PLUS_4096: missing next_byte at position {}",
                            input_pos
                        ),
                    });
                }
                let next_byte = input[input_pos] as usize;
                input_pos += 1;
                let count = next_byte + 64 + length * 256 + 4096;
                copy_bytes(input, &mut input_pos, &mut output, count, output_length)?;
            }

            // COPY96: Copy length + 96 bytes
            0x2 => {
                let count = length + 96;
                copy_bytes(input, &mut input_pos, &mut output, count, output_length)?;
            }

            // 0x3 is undefined/unused
            0x3 => {
                return Err(SasError::DecompressionError {
                    page_index: 0,
                    message: format!("Unknown RLE command 0x3 at position {}", input_pos - 1),
                });
            }

            // INSERT_BYTE18: Repeat next byte next_byte + 18 + length*256 times
            0x4 => {
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "INSERT_BYTE18: missing next_byte at position {}",
                            input_pos
                        ),
                    });
                }
                let next_byte = input[input_pos];
                input_pos += 1;
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "INSERT_BYTE18: missing fill_byte at position {}",
                            input_pos
                        ),
                    });
                }
                let fill_byte = input[input_pos];
                input_pos += 1;
                let count = next_byte as usize + 18 + length * 256;
                repeat_byte(&mut output, fill_byte, count, output_length)?;
            }

            // INSERT_AT17: Insert next_byte + 17 + length*256 '@' chars (0x40)
            0x5 => {
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "INSERT_AT17: missing next_byte at position {}",
                            input_pos
                        ),
                    });
                }
                let next_byte = input[input_pos] as usize;
                input_pos += 1;
                let count = next_byte + 17 + length * 256;
                repeat_byte(&mut output, 0x40, count, output_length)?;
            }

            // INSERT_BLANK17: Insert next_byte + 17 + length*256 spaces (0x20)
            0x6 => {
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "INSERT_BLANK17: missing next_byte at position {}",
                            input_pos
                        ),
                    });
                }
                let next_byte = input[input_pos] as usize;
                input_pos += 1;
                let count = next_byte + 17 + length * 256;
                repeat_byte(&mut output, 0x20, count, output_length)?;
            }

            // INSERT_ZERO17: Insert next_byte + 17 + length*256 null bytes (0x00)
            0x7 => {
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "INSERT_ZERO17: missing next_byte at position {}",
                            input_pos
                        ),
                    });
                }
                let next_byte = input[input_pos] as usize;
                input_pos += 1;
                let count = next_byte + 17 + length * 256;
                repeat_byte(&mut output, 0x00, count, output_length)?;
            }

            // COPY1: Copy length + 1 bytes
            0x8 => {
                let count = length + 1;
                copy_bytes(input, &mut input_pos, &mut output, count, output_length)?;
            }

            // COPY17: Copy length + 17 bytes
            0x9 => {
                let count = length + 17;
                copy_bytes(input, &mut input_pos, &mut output, count, output_length)?;
            }

            // COPY33: Copy length + 33 bytes
            0xA => {
                let count = length + 33;
                copy_bytes(input, &mut input_pos, &mut output, count, output_length)?;
            }

            // COPY49: Copy length + 49 bytes
            0xB => {
                let count = length + 49;
                copy_bytes(input, &mut input_pos, &mut output, count, output_length)?;
            }

            // INSERT_BYTE3: Repeat next byte length + 3 times
            0xC => {
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "INSERT_BYTE3: missing fill_byte at position {}",
                            input_pos
                        ),
                    });
                }
                let fill_byte = input[input_pos];
                input_pos += 1;
                let count = length + 3;
                repeat_byte(&mut output, fill_byte, count, output_length)?;
            }

            // INSERT_AT2: Insert length + 2 '@' chars (0x40)
            0xD => {
                let count = length + 2;
                repeat_byte(&mut output, 0x40, count, output_length)?;
            }

            // INSERT_BLANK2: Insert length + 2 spaces (0x20)
            0xE => {
                let count = length + 2;
                repeat_byte(&mut output, 0x20, count, output_length)?;
            }

            // INSERT_ZERO2: Insert length + 2 null bytes (0x00)
            0xF => {
                let count = length + 2;
                repeat_byte(&mut output, 0x00, count, output_length)?;
            }

            _ => {
                return Err(SasError::DecompressionError {
                    page_index: 0,
                    message: format!(
                        "Unknown RLE command 0x{:X} at position {}",
                        command,
                        input_pos - 1
                    ),
                });
            }
        }
    }

    if output.len() != output_length {
        return Err(SasError::DecompressionError {
            page_index: 0,
            message: format!(
                "Output length mismatch: expected {}, got {}",
                output_length,
                output.len()
            ),
        });
    }

    Ok(output)
}

/// Decompress RDC-compressed data.
///
/// RDC uses a hybrid compression scheme:
/// - 16-bit control words where each bit indicates literal (0) or command (1)
/// - Commands encode run-length sequences and back-references (LZ77-style)
///
/// # Arguments
///
/// * `input` - Compressed byte stream
/// * `output_length` - Expected decompressed size in bytes
///
/// # Returns
///
/// Decompressed byte vector of exactly `output_length` bytes.
///
/// # Errors
///
/// Returns `SasError::DecompressionError` if:
/// - Input is exhausted prematurely
/// - Output buffer overflow occurs
/// - Back-reference offset is invalid
///
/// # Reference
///
/// Based on Parso BinDecompressor.java RDC implementation.
pub fn decompress_rdc(input: &[u8], output_length: usize) -> Result<Vec<u8>, SasError> {
    let mut output = Vec::with_capacity(output_length);
    let mut input_pos = 0;

    while output.len() < output_length {
        // Read 16-bit control word (big-endian)
        if input_pos + 1 >= input.len() {
            return Err(SasError::DecompressionError {
                page_index: 0,
                message: format!(
                    "RDC: premature end of input reading control word at position {}",
                    input_pos
                ),
            });
        }

        let control_bits = u16::from_be_bytes([input[input_pos], input[input_pos + 1]]);
        input_pos += 2;

        // Process 16 control bits (MSB first: bit 15 down to bit 0)
        for bit_index in (0..16).rev() {
            if output.len() >= output_length {
                break;
            }

            let bit = (control_bits >> bit_index) & 1;

            if bit == 0 {
                // Literal byte: copy one byte from input to output
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "RDC: premature end of input reading literal byte at position {}",
                            input_pos
                        ),
                    });
                }
                output.push(input[input_pos]);
                input_pos += 1;
            } else {
                // Command byte
                if input_pos >= input.len() {
                    return Err(SasError::DecompressionError {
                        page_index: 0,
                        message: format!(
                            "RDC: premature end of input reading command byte at position {}",
                            input_pos
                        ),
                    });
                }

                let command_byte = input[input_pos];
                input_pos += 1;

                let cmd = (command_byte >> 4) & 0x0F;
                let cnt = (command_byte & 0x0F) as usize;

                match cmd {
                    // Short RLE
                    0 => {
                        if input_pos >= input.len() {
                            return Err(SasError::DecompressionError {
                                page_index: 0,
                                message: format!(
                                    "RDC: short RLE missing fill_byte at position {}",
                                    input_pos
                                ),
                            });
                        }
                        let fill_byte = input[input_pos];
                        input_pos += 1;
                        let count = cnt + 3;
                        repeat_byte(&mut output, fill_byte, count, output_length)?;
                    }

                    // Long RLE
                    // Reference: Parso BinDecompressor.java / pandas sas.pyx
                    // First byte after command extends cnt, second byte is the fill value.
                    1 => {
                        if input_pos + 1 >= input.len() {
                            return Err(SasError::DecompressionError {
                                page_index: 0,
                                message: format!(
                                    "RDC: long RLE missing length_byte/fill_byte at position {}",
                                    input_pos
                                ),
                            });
                        }
                        let length_byte = input[input_pos] as usize;
                        input_pos += 1;
                        let count = cnt + (length_byte << 4) + 19;
                        let fill_byte = input[input_pos];
                        input_pos += 1;
                        repeat_byte(&mut output, fill_byte, count, output_length)?;
                    }

                    // Long Pattern (back-reference)
                    // Reference: Parso BinDecompressor.java / pandas sas.pyx
                    // offset = cnt + 3 + (next_byte << 4), count = length_byte + 16
                    2 => {
                        if input_pos + 1 >= input.len() {
                            return Err(SasError::DecompressionError {
                                page_index: 0,
                                message: format!(
                                    "RDC: long pattern missing offset/length bytes at position {}",
                                    input_pos
                                ),
                            });
                        }
                        let next_byte = input[input_pos] as usize;
                        input_pos += 1;
                        let offset = cnt + 3 + (next_byte << 4);

                        let length_byte = input[input_pos] as usize;
                        input_pos += 1;
                        let count = length_byte + 16;

                        copy_from_output(&mut output, offset, count, output_length)?;
                    }

                    // Short Pattern (back-reference)
                    // Reference: Parso BinDecompressor.java / pandas sas.pyx
                    // offset = cnt + 3 + (next_byte << 4), count = cmd
                    cmd @ 3..=15 => {
                        if input_pos >= input.len() {
                            return Err(SasError::DecompressionError {
                                page_index: 0,
                                message: format!(
                                    "RDC: short pattern missing offset byte at position {}",
                                    input_pos
                                ),
                            });
                        }
                        let next_byte = input[input_pos] as usize;
                        input_pos += 1;
                        let offset = cnt + 3 + (next_byte << 4);
                        let count = cmd as usize;

                        copy_from_output(&mut output, offset, count, output_length)?;
                    }

                    // This should be unreachable due to cmd being 4 bits (0-15)
                    _ => unreachable!(),
                }
            }
        }
    }

    if output.len() != output_length {
        return Err(SasError::DecompressionError {
            page_index: 0,
            message: format!(
                "RDC: output length mismatch: expected {}, got {}",
                output_length,
                output.len()
            ),
        });
    }

    Ok(output)
}

/// Copy bytes from input to output buffer.
///
/// # Arguments
///
/// * `input` - Source byte slice
/// * `input_pos` - Current position in input (mutated)
/// * `output` - Destination buffer
/// * `count` - Number of bytes to copy
/// * `output_length` - Maximum allowed output length
///
/// # Errors
///
/// Returns `SasError::DecompressionError` if input is exhausted or output would overflow.
fn copy_bytes(
    input: &[u8],
    input_pos: &mut usize,
    output: &mut Vec<u8>,
    count: usize,
    output_length: usize,
) -> Result<(), SasError> {
    if output.len() + count > output_length {
        return Err(SasError::DecompressionError {
            page_index: 0,
            message: format!(
                "Output buffer overflow: trying to write {} bytes, but only {} bytes remaining (output {} of {})",
                count,
                output_length - output.len(),
                output.len(),
                output_length
            ),
        });
    }

    if *input_pos + count > input.len() {
        return Err(SasError::DecompressionError {
            page_index: 0,
            message: format!(
                "Premature end of input: trying to read {} bytes at position {}, but only {} bytes remaining",
                count,
                *input_pos,
                input.len() - *input_pos
            ),
        });
    }

    output.extend_from_slice(&input[*input_pos..*input_pos + count]);
    *input_pos += count;

    Ok(())
}

/// Repeat a byte `count` times into the output buffer.
///
/// # Arguments
///
/// * `output` - Destination buffer
/// * `byte` - Byte value to repeat
/// * `count` - Number of repetitions
/// * `output_length` - Maximum allowed output length
///
/// # Errors
///
/// Returns `SasError::DecompressionError` if output would overflow.
fn repeat_byte(
    output: &mut Vec<u8>,
    byte: u8,
    count: usize,
    output_length: usize,
) -> Result<(), SasError> {
    if output.len() + count > output_length {
        return Err(SasError::DecompressionError {
            page_index: 0,
            message: format!(
                "Output buffer overflow: trying to write {} bytes, but only {} bytes remaining (output {} of {})",
                count,
                output_length - output.len(),
                output.len(),
                output_length
            ),
        });
    }

    output.extend(std::iter::repeat_n(byte, count));
    Ok(())
}

/// Copy bytes from earlier in the output buffer (back-reference / LZ77-style).
///
/// Handles overlapping copy where source and destination regions overlap.
/// Copies byte-by-byte to allow overlapping regions (e.g., offset=1, count=10
/// will repeat the previous byte 10 times).
///
/// # Arguments
///
/// * `output` - Output buffer (both source and destination)
/// * `offset` - Distance back from current position to start copying
/// * `count` - Number of bytes to copy
/// * `output_length` - Maximum allowed output length
///
/// # Errors
///
/// Returns `SasError::DecompressionError` if offset is invalid or output would overflow.
fn copy_from_output(
    output: &mut Vec<u8>,
    offset: usize,
    count: usize,
    output_length: usize,
) -> Result<(), SasError> {
    if offset > output.len() {
        return Err(SasError::DecompressionError {
            page_index: 0,
            message: format!(
                "Invalid back-reference: offset {} exceeds current output position {}",
                offset,
                output.len()
            ),
        });
    }

    if offset == 0 {
        return Err(SasError::DecompressionError {
            page_index: 0,
            message: "Invalid back-reference: offset cannot be zero".to_string(),
        });
    }

    if output.len() + count > output_length {
        return Err(SasError::DecompressionError {
            page_index: 0,
            message: format!(
                "Output buffer overflow: trying to write {} bytes, but only {} bytes remaining (output {} of {})",
                count,
                output_length - output.len(),
                output.len(),
                output_length
            ),
        });
    }

    let start_pos = output.len() - offset;

    // Copy byte-by-byte to handle overlapping regions
    for i in 0..count {
        let byte = output[start_pos + i];
        output.push(byte);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rle_copy1() {
        // Command 0x8, length 2 → Copy 3 bytes
        let input = vec![0x82, b'A', b'B', b'C'];
        let result = decompress_rle(&input, 3).unwrap();
        assert_eq!(result, b"ABC");
    }

    #[test]
    fn test_rle_insert_byte3() {
        // Command 0xC, length 2 → Repeat next byte 5 times
        let input = vec![0xC2, b'X'];
        let result = decompress_rle(&input, 5).unwrap();
        assert_eq!(result, b"XXXXX");
    }

    #[test]
    fn test_rle_insert_blank2() {
        // Command 0xE, length 3 → Insert 5 spaces
        let input = vec![0xE3];
        let result = decompress_rle(&input, 5).unwrap();
        assert_eq!(result, b"     ");
    }

    #[test]
    fn test_rle_insert_zero2() {
        // Command 0xF, length 1 → Insert 3 null bytes
        let input = vec![0xF1];
        let result = decompress_rle(&input, 3).unwrap();
        assert_eq!(result, vec![0, 0, 0]);
    }

    #[test]
    fn test_rle_insert_at2() {
        // Command 0xD, length 0 → Insert 2 '@' chars
        let input = vec![0xD0];
        let result = decompress_rle(&input, 2).unwrap();
        assert_eq!(result, b"@@");
    }

    #[test]
    fn test_rle_multiple_commands() {
        // COPY1 with length=1 (2 bytes) + INSERT_BYTE3 with length=0 (3 times 'X')
        let input = vec![0x81, b'A', b'B', 0xC0, b'X'];
        let result = decompress_rle(&input, 5).unwrap();
        assert_eq!(result, b"ABXXX");
    }

    #[test]
    fn test_rle_unknown_command() {
        // Command 0x3 is undefined
        let input = vec![0x30];
        let result = decompress_rle(&input, 10);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown RLE command 0x3"));
    }

    #[test]
    fn test_rle_premature_end() {
        // COPY1 requires 1 byte after control byte
        let input = vec![0x80]; // Length 0 → 1 byte needed
        let result = decompress_rle(&input, 1);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Premature end of input"));
    }

    #[test]
    fn test_rle_buffer_overflow() {
        // Try to write 3 bytes but expect only 2
        let input = vec![0x82, b'A', b'B', b'C'];
        let result = decompress_rle(&input, 2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("overflow"));
    }

    #[test]
    fn test_rdc_literal_bytes() {
        // Control word: 0x0000 (all literal)
        // Next 16 bytes are literals
        let mut input = vec![0x00, 0x00];
        input.extend_from_slice(b"Hello, World!!!!");
        let result = decompress_rdc(&input, 16).unwrap();
        assert_eq!(result, b"Hello, World!!!!");
    }

    #[test]
    fn test_rdc_short_rle() {
        // Control word: 0x8000 (bit 15 = 1, rest = 0)
        // Command: 0x05 (cmd=0, cnt=5) → repeat next byte 8 times
        // Fill byte: 'X'
        let input = vec![0x80, 0x00, 0x05, b'X'];
        let result = decompress_rdc(&input, 8).unwrap();
        assert_eq!(result, b"XXXXXXXX");
    }

    #[test]
    fn test_rdc_long_rle() {
        // Control word: 0x8000 (bit 15 = 1)
        // Command: 0x12 (cmd=1, cnt=2) → long RLE
        // Per Ross algorithm: first byte after cmd is length extension, second is fill byte
        // length_byte: 0x01, fill_byte: 'A'
        // Count = 2 + (1 << 4) + 19 = 37
        let input = vec![0x80, 0x00, 0x12, 0x01, b'A'];
        let result = decompress_rdc(&input, 37).unwrap();
        assert_eq!(result.len(), 37);
        assert!(result.iter().all(|&b| b == b'A'));
    }

    #[test]
    fn test_rdc_back_reference() {
        // First write 3 literals "ABC", then back-reference to copy them again
        // Control word: 0x1000 (binary: 0001 0000 0000 0000)
        // Bit 15=0: literal 'A'
        // Bit 14=0: literal 'B'
        // Bit 13=0: literal 'C'
        // Bit 12=1: command → 0x30 (cmd=3, cnt=0) → count=3
        // Offset = cnt + 3 + (next_byte << 4) = 0 + 3 + (0 << 4) = 3
        // Offset byte: 0x00 → copies last 3 bytes
        // Output reaches 6 bytes (ABCABC), loop exits
        let input = vec![0x10, 0x00, b'A', b'B', b'C', 0x30, 0x00];
        let result = decompress_rdc(&input, 6).unwrap();
        assert_eq!(result, b"ABCABC");
    }

    #[test]
    fn test_rdc_overlapping_back_reference() {
        // Write "AAA" as 3 literals, then back-reference with offset=3, count=5
        // to repeat "AAA" overlapping (copies byte-by-byte from start)
        // Control word: 0x1000 (binary: 0001 0000 0000 0000)
        // Bit 15=0: literal 'A'
        // Bit 14=0: literal 'A'
        // Bit 13=0: literal 'A'
        // Bit 12=1: command → 0x50 (cmd=5, cnt=0) → count=5
        // Offset = cnt + 3 + (next_byte << 4) = 0 + 3 + (0 << 4) = 3
        // Offset byte: 0x00 → copies from 3 back overlapping
        // Output: "AAA" + 5 more 'A's = "AAAAAAAA" (8 bytes)
        let input = vec![0x10, 0x00, b'A', b'A', b'A', 0x50, 0x00];
        let result = decompress_rdc(&input, 8).unwrap();
        assert_eq!(result, b"AAAAAAAA");
    }

    #[test]
    fn test_rdc_invalid_offset() {
        // Try to back-reference beyond start of output
        // Control word: 0x8000 (first bit=1, command immediately)
        // Command: 0x30 (cmd=3, cnt=0) → count=3
        // Offset = cnt + 3 + (next_byte << 4) = 0 + 3 + (0x0A << 4) = 163
        // Output is empty so any offset > 0 is invalid
        let input = vec![0x80, 0x00, 0x30, 0x0A];
        let result = decompress_rdc(&input, 3);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid back-reference"));
    }

    #[test]
    fn test_rdc_premature_end() {
        // Control word with no following data
        let input = vec![0x80, 0x00];
        let result = decompress_rdc(&input, 10);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("premature end of input"));
    }

    #[test]
    fn test_copy_from_output_zero_offset() {
        let mut output = vec![b'A', b'B', b'C'];
        let result = copy_from_output(&mut output, 0, 3, 10);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("offset cannot be zero"));
    }

    #[test]
    fn test_repeat_byte_overflow() {
        let mut output = Vec::new();
        let result = repeat_byte(&mut output, b'X', 10, 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("overflow"));
    }

    #[test]
    fn test_rdc_short_pattern_with_nonzero_cnt() {
        // Verify offset = cnt + 3 + (next_byte << 4) with cnt > 0
        // Write 20 literals, then back-reference with larger offset
        // We need 20 literal bytes first, which takes 2 control words:
        //   Control word 1: 0x0000 (16 literal bits) -> 16 literals
        //   Control word 2: 0x0800 (bits 15..12 = 0000, bit 11 = 1, rest = 0)
        //     -> 4 literals then command
        //
        // Command byte: 0x34 (cmd=3, cnt=4) with offset_byte=0x01
        // Offset = 4 + 3 + (1 << 4) = 23
        // Count = 3 (cmd value)
        // At output position 20, offset=23 is too large (> 20)
        // So use offset_byte=0x00: offset = 4 + 3 + 0 = 7, copy 3 bytes from pos 13
        let mut input = vec![0x00, 0x00]; // ctrl word 1: all literals
        input.extend_from_slice(b"ABCDEFGHIJKLMNOP"); // 16 literals
        input.extend_from_slice(&[0x08, 0x00]); // ctrl word 2: 4 lits then cmd
        input.extend_from_slice(b"QRST"); // 4 more literals (total 20: A..T)
        input.push(0x34); // cmd=3, cnt=4
        input.push(0x00); // offset_byte=0 -> offset = 4+3+0 = 7
        // At position 20, offset 7 -> copies from position 13 = "NOP" (3 bytes)
        let result = decompress_rdc(&input, 23).unwrap();
        assert_eq!(&result[..20], b"ABCDEFGHIJKLMNOPQRST");
        assert_eq!(&result[20..23], b"NOP");
    }

    #[test]
    fn test_rdc_long_pattern_back_reference() {
        // Test cmd=2 (long pattern): offset = cnt + 3 + (next_byte << 4), count = length_byte + 16
        // First emit 20 literals, then use long pattern to copy 16+ bytes
        // Control word 1: 0x0000 -> 16 literals
        // Control word 2: 0x0800 -> 4 literals, then command at bit 11
        // Command: 0x20 (cmd=2, cnt=0)
        //   offset_byte: 0x01 -> offset = 0 + 3 + (1 << 4) = 19
        //   length_byte: 0x00 -> count = 0 + 16 = 16
        //   At output pos 20, offset 19 -> copies from position 1 (B..Q, 16 bytes)
        let mut input = vec![0x00, 0x00]; // ctrl word 1
        input.extend_from_slice(b"ABCDEFGHIJKLMNOP"); // 16 literals
        input.extend_from_slice(&[0x08, 0x00]); // ctrl word 2
        input.extend_from_slice(b"QRST"); // 4 more literals
        input.push(0x20); // cmd=2, cnt=0
        input.push(0x01); // offset_byte -> offset = 0 + 3 + 16 = 19
        input.push(0x00); // length_byte -> count = 0 + 16 = 16
        let result = decompress_rdc(&input, 36).unwrap();
        assert_eq!(&result[..20], b"ABCDEFGHIJKLMNOPQRST");
        // From position 1 (offset 19 from pos 20), copies BCDEFGHIJKLMNOPQ (16 bytes)
        assert_eq!(&result[20..36], b"BCDEFGHIJKLMNOPQ");
    }

    #[test]
    fn test_rdc_offset_with_shifted_next_byte() {
        // Verify the << 4 shift in the offset formula
        // Write 100 literals (7 control words), then back-reference with next_byte > 0
        // 6 full control words of 16 literals = 96 bytes, then 1 ctrl word with 4 lits + cmd
        let mut input = Vec::new();
        // 6 control words of all-literal (96 bytes)
        for _ in 0..6 {
            input.extend_from_slice(&[0x00, 0x00]);
            input.extend_from_slice(&[b'X'; 16]);
        }
        // 7th control word: 4 literals then command at bit 11
        input.extend_from_slice(&[0x08, 0x00]);
        input.extend_from_slice(&[b'Y'; 4]); // 4 literals -> total 100 bytes
        // Command: 0x30 (cmd=3, cnt=0), offset_byte=0x05
        // offset = 0 + 3 + (5 << 4) = 0 + 3 + 80 = 83
        // At position 100, offset 83 -> copies from position 17, 3 bytes
        input.push(0x30); // cmd=3, cnt=0
        input.push(0x05); // offset_byte=5 -> offset = 83
        let result = decompress_rdc(&input, 103).unwrap();
        assert_eq!(result.len(), 103);
        // Position 17 is in the second group of X's -> "XXX"
        assert_eq!(&result[100..103], b"XXX");
    }
}
