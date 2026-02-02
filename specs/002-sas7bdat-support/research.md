# Research: SAS7BDAT File Format Support

**Date:** 2026-02-01
**Feature:** 002-sas7bdat-support

---

## Decision 1: Build vs Buy — SAS7BDAT Parser

### Decision
Build a custom pure Rust SAS7BDAT parser as a module within Lo-phi (`src/pipeline/sas7bdat/`).

### Rationale
- **No viable pure Rust crate exists.** The only candidate, `quick-sas7bdat` (tafia), has 8 commits from 2017, zero functionality ("nothing is working yet"), and is abandoned.
- **`readstat-rs`** wraps the ReadStat C library via FFI (`readstat-sys` + `bindgen`), violating NFR-1 (pure Rust, no C FFI). It also rounds floating-point values to 14 digits, violating NFR-3 (bit-identical numeric fidelity).
- **Reference implementations exist** in Python (pandas `sas7bdat.py`) and Java (Parso `SasFileParser.java`), both well-documented and Apache-2.0 licensed, suitable as algorithmic references.
- The format is reverse-engineered but stable — the Shotwell spec and multiple mature implementations provide sufficient documentation.

### Alternatives Considered
| Option | Pros | Cons |
|--------|------|------|
| `quick-sas7bdat` crate | Pure Rust | Dead project (2017), no functionality |
| `readstat-rs` crate | Feature-complete, battle-tested | C FFI dependency, precision loss, complex build chain |
| Polars built-in | Zero effort | Polars has no SAS7BDAT reader |
| Shell out to Python | Quick to prototype | Runtime Python dependency, performance, deployment complexity |
| Build custom parser | Full control, pure Rust, bit-accurate | Development effort |

---

## Decision 2: Binary Format Reference Sources

### Decision
Use the following as authoritative references (in priority order):
1. **Pandas `sas_constants.py` + `sas7bdat.py`** — Most complete Python implementation, well-tested
2. **Parso (EPAM) Java implementation** — Apache 2.0 license, has RDC decompression
3. **Shotwell specification** (BioStatMatt/sas7bdat) — Original reverse-engineering documentation
4. **ReadStat C library** — RLE decompression reference (`readstat_sas_rle.c`)

### Rationale
Cross-referencing multiple implementations reduces risk of format misinterpretation. Each source has strengths: pandas for constants/offsets, Parso for RDC, ReadStat for RLE, Shotwell for structural documentation.

---

## Decision 3: SAS7BDAT Binary Format Key Parameters

### Magic Number (32 bytes)
```
\x00\x00\x00\x00\x00\x00\x00\x00
\x00\x00\x00\x00\xc2\xea\x81\x60
\xb3\x14\x11\xcf\xbd\x92\x08\x00
\x09\xc7\x31\x8c\x18\x1f\x10\x11
```

### Header Layout
- **Alignment detection**: Byte at offset 32 — value `0x33` indicates 64-bit format
- **Endianness**: Byte at offset 37 — `0x01` = little-endian, `0x00` = big-endian
- **Encoding**: 2 bytes at offset 70 — maps to encoding table (20=UTF-8, 29=Latin-1, etc.)
- **Timestamps**: Doubles at offsets 164+a1 and 172+a1 (seconds since 1960-01-01)
- **Page size**: 4 bytes at offset 200+a1
- **Page count**: 4|8 bytes at offset 204+a1 (8 bytes for 64-bit files)

### Page Types
| Value | Type | Content |
|-------|------|---------|
| 0x0000 | Meta | Subheaders only |
| 0x0100 | Data | Packed binary rows only |
| 0x0200 | Mix | Subheaders + data rows |
| 0x0400 | AMD | Amended metadata |
| 0x4000 | Meta2 | Additional metadata |
| 0x9000 | Comp | Compressed page |

### Subheader Signatures (32-bit / 64-bit)
| Signature | Type |
|-----------|------|
| `F7F7F7F7` / `F7F7F7F700000000` | Row Size |
| `F6F6F6F6` / `F6F6F6F600000000` | Column Size |
| `00FCFFFF` / `00FCFFFFFFFFFFFF` | Subheader Counts |
| `FDFFFFFF` / `FDFFFFFFFFFFFFFF` | Column Text |
| `FFFFFFFF` / `FFFFFFFFFFFFFFFF` | Column Name |
| `FCFFFFFF` / `FCFFFFFFFFFFFFFF` | Column Attributes |
| `FEFBFFFF` / `FEFBFFFFFFFFFFFF` | Format and Label |
| `FEFFFFFF` / `FEFFFFFFFFFFFFFF` | Column List |

### Compression Identifiers
- RLE (COMPRESS=CHAR): `SASYZCRL` (8 bytes in column text block)
- RDC (COMPRESS=BINARY): `SASYZCR2` (8 bytes in column text block)

---

## Decision 4: RLE Decompression Algorithm

### Decision
Implement RLE decompression based on the ReadStat C implementation (`readstat_sas_rle.c`).

### Algorithm
Control byte format: `[command(4 bits) | length(4 bits)]`
- Command = `(control & 0xF0) >> 4`
- Length = `(control & 0x0F)`

| Command | Hex | Operation |
|---------|-----|-----------|
| COPY64 | 0x0 | Copy `next_byte + 64 + length*256` bytes from input |
| COPY64_PLUS_4096 | 0x1 | Copy `next_byte + 64 + length*256 + 4096` bytes |
| COPY96 | 0x2 | Copy `length + 96` bytes from input |
| INSERT_BYTE18 | 0x4 | Repeat next byte `next_byte + 18 + length*256` times |
| INSERT_AT17 | 0x5 | Insert `next_byte + 17 + length*256` `@` chars (0x40) |
| INSERT_BLANK17 | 0x6 | Insert `next_byte + 17 + length*256` spaces (0x20) |
| INSERT_ZERO17 | 0x7 | Insert `next_byte + 17 + length*256` null bytes (0x00) |
| COPY1 | 0x8 | Copy `length + 1` bytes from input |
| COPY17 | 0x9 | Copy `length + 17` bytes from input |
| COPY33 | 0xA | Copy `length + 33` bytes from input |
| COPY49 | 0xB | Copy `length + 49` bytes from input |
| INSERT_BYTE3 | 0xC | Repeat next byte `length + 3` times |
| INSERT_AT2 | 0xD | Insert `length + 2` `@` chars (0x40) |
| INSERT_BLANK2 | 0xE | Insert `length + 2` spaces (0x20) |
| INSERT_ZERO2 | 0xF | Insert `length + 2` null bytes (0x00) |

### Rationale
ReadStat is the most complete RLE reference with explicit command documentation. The control byte structure is well-understood and consistent across all reference implementations.

---

## Decision 5: RDC Decompression Algorithm

### Decision
Implement RDC decompression based on the Parso Java implementation (`BinDecompressor.java`).

### Algorithm
RDC uses a 16-bit control word with sliding window back-references:

1. Read 16-bit control word (bitwise flags for next 16 blocks)
2. For each bit in the control word:
   - **Bit = 0**: Copy one literal byte from input to output
   - **Bit = 1**: Read command byte, extract command type from upper 2 bits:
     - **cmd=0 (Short RLE)**: Repeat single byte `count + 3` times
     - **cmd=1 (Long RLE)**: Repeat single byte `count + next_byte*256 + 19` times
     - **cmd=2 (Long Pattern)**: Back-reference with offset computed from count + next_byte, length 16+ bytes
     - **cmd=3 (Short Pattern)**: Back-reference with offset, length 3-15 bytes from command nibble

### Rationale
Parso is the most readable RDC implementation (Java vs C). The algorithm is an LZ77-family sliding window compressor, well-understood but less commonly encountered in SAS files than RLE.

---

## Decision 6: Character Encoding Handling

### Decision
Use the `encoding_rs` crate for character encoding conversion.

### Rationale
- Pure Rust (Apache-2.0/MIT/BSD-3 licensed)
- Implements WHATWG Encoding Standard
- Covers all major SAS encodings: UTF-8, Latin-1, Windows-1252, Shift-JIS, EUC-JP, EUC-KR, Big5, GBK, GB18030
- Zero-allocation API available for performance
- The `mem` module provides efficient Latin-1 ↔ UTF-8 conversion ("isomorphic decode")
- Already a well-maintained, battle-tested crate used by Firefox/Servo

### SAS Encoding ID → encoding_rs Mapping (key values)
| SAS ID | Encoding | encoding_rs constant |
|--------|----------|---------------------|
| 20 | UTF-8 | `UTF_8` |
| 28 | US-ASCII | `UTF_8` (ASCII is valid UTF-8) |
| 29 | ISO-8859-1 | `mem::decode_latin1` / `WINDOWS_1252` |
| 62 | Windows-1252 | `WINDOWS_1252` |
| 125 | EUC-CN | `GBK` |
| 134 | EUC-JP | `EUC_JP` |
| 138 | Shift-JIS | `SHIFT_JIS` |
| 140 | EUC-KR | `EUC_KR` |

### Alternatives Considered
| Option | Pros | Cons |
|--------|------|------|
| `encoding_rs` | Full encoding coverage, pure Rust, maintained | None significant |
| `encoding` crate | Older, similar coverage | Less maintained than encoding_rs |
| Manual Latin-1 conversion only | Zero dependencies | Insufficient for CJK encodings |

---

## Decision 7: SAS Date/Datetime Epoch Conversion

### Decision
Use compile-time constants for epoch offset conversion.

### Key Constants
- **SAS epoch**: January 1, 1960, 00:00:00
- **Unix epoch**: January 1, 1970, 00:00:00
- **Offset**: 3,653 days = 315,619,200 seconds

### Conversion Formulas
- **SAS DATE (days since 1960-01-01) → Polars Date**:
  `polars_days = sas_days - 3653` (days since Unix epoch)
- **SAS DATETIME (seconds since 1960-01-01 00:00) → Polars Datetime**:
  `polars_ms = (sas_seconds - 315_619_200) * 1000` (milliseconds since Unix epoch)
- **SAS TIME (seconds since midnight) → Polars Time**:
  `polars_ns = sas_seconds * 1_000_000_000` (nanoseconds since midnight)

### SAS Date Format Detection
Detect via column format string in Format/Label subheader:
| SAS Format Pattern | Polars Type |
|-------------------|-------------|
| `DATE`, `DDMMYY`, `MMDDYY`, `YYMMDD` + width variants | `Date` |
| `DATETIME` + width variants | `Datetime(Milliseconds, None)` |
| `TIME` + width variants | `Time` |
| All other formats | `Float64` (raw numeric) |

Format matching should be case-insensitive and strip width/decimal suffixes (e.g., `DATE9.` → `DATE`, `DATETIME20.` → `DATETIME`).

---

## Decision 8: SAS Missing Value Handling

### Decision
Map all SAS missing value sentinels to `null` in Polars.

### SAS Missing Value Encoding
SAS encodes missing values as special IEEE 754 double-precision NaN values:
- **Standard missing (`.`)**: First byte is the missing indicator
- **Special missing (`.A` through `.Z`, `._`)**: First byte encodes the letter/underscore
- All missing values have bytes 2-8 as `0x00`

### Detection Logic
For numeric columns: check if the first byte of the 8-byte double matches a missing sentinel pattern. If so, emit `null` instead of attempting IEEE 754 conversion.

For character columns: all-spaces or all-nulls strings are NOT treated as missing (SAS distinguishes empty strings from missing). Only truly missing character values (no data in the row) map to `null`.

### Rationale
Polars uses `null` for missing values across all types. Mapping SAS missing sentinels to `null` preserves the existing pipeline behavior (missing value analysis counts nulls).

---

## Decision 9: Truncated Numeric Representation

### Decision
Implement proper handling of SAS truncated numeric storage (3-8 bytes).

### Background
SAS can store numeric values in fewer than 8 bytes to save space. The stored bytes are the most significant bytes of an IEEE 754 double. To reconstruct:
1. Create an 8-byte buffer initialized to `0x00`
2. Copy the stored N bytes (3-8) into the **most significant** positions (big-endian) or appropriate positions based on endianness
3. Interpret the 8-byte buffer as an IEEE 754 double

### Rationale
This is critical for NFR-3 (numeric fidelity). Many SAS files use `LENGTH` statements to store integers in 3-4 bytes. Incorrect reconstruction silently corrupts data.

---

## Decision 10: Module Architecture

### Decision
Organize the SAS7BDAT parser as a submodule of `src/pipeline/` with the following structure:

```
src/pipeline/sas7bdat/
├── mod.rs           // Public API: load_sas7bdat(), get_sas7bdat_columns()
├── header.rs        // Header parsing, alignment, endianness detection
├── page.rs          // Page iteration, page type handling
├── subheader.rs     // Subheader parsing (row size, column size, etc.)
├── column.rs        // Column metadata extraction (name, type, format)
├── decompress.rs    // RLE and RDC decompression
├── data.rs          // Row data extraction, type conversion
├── constants.rs     // Magic number, signatures, encoding table, offsets
└── error.rs         // SAS-specific error types
```

### Rationale
- Consistent with existing module structure (e.g., `src/pipeline/iv.rs` at ~2600 lines shows the project doesn't shy from substantial modules, but SAS7BDAT parsing benefits from separation of concerns)
- Each file maps to a distinct parsing phase, making testing granular
- `constants.rs` centralizes all format-specific magic values
- `error.rs` provides structured error types for NFR-4 (clear error reporting)

### Alternatives Considered
| Option | Pros | Cons |
|--------|------|------|
| Single file (`sas7bdat.rs`) | Simple, one file to find things | Would exceed 2000+ lines |
| Separate crate | Clean boundary, reusable | Over-engineering for Lo-phi's needs |
| Submodule (chosen) | Organized, testable, right-sized | Slightly more files |
