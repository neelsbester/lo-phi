# Quickstart: SAS7BDAT File Format Support

**Date:** 2026-02-01

---

## Prerequisites

- Rust toolchain (stable)
- `encoding_rs` crate added to `Cargo.toml`

## Implementation Order

```
Phase 1: constants.rs → error.rs → header.rs → page.rs → subheader.rs → column.rs
Phase 2: decompress.rs → data.rs → mod.rs (public API)
Phase 3: loader.rs → config_menu.rs → args.rs → convert.rs → main.rs
Phase 4: test fixtures → unit tests → integration tests
Phase 5: Cargo.toml → CLAUDE.md → docs → ADR-009
```

## Getting Started

### Step 1: Add dependency

```toml
# Cargo.toml
encoding_rs = "0.8"
```

### Step 2: Create module skeleton

```bash
mkdir -p src/pipeline/sas7bdat
touch src/pipeline/sas7bdat/{mod,constants,error,header,page,subheader,column,decompress,data}.rs
```

### Step 3: Start with constants.rs

Define the magic number, subheader signatures, page types, and encoding table.
This file has zero dependencies and is imported by every other module.

### Step 4: Build up incrementally

Each module depends only on `constants.rs` and/or the previous module.
Write unit tests as you go — each module is independently testable.

### Step 5: Integration points (minimal changes to existing code)

| File | Change |
|------|--------|
| `src/pipeline/mod.rs` | Add `pub mod sas7bdat;` |
| `src/pipeline/loader.rs` | Add `"sas7bdat"` match arm (2 locations) |
| `src/cli/config_menu.rs` | Add to `is_valid_data_file()` filter |
| `src/cli/args.rs` | Update help text strings |
| `src/cli/convert.rs` | Add SAS7BDAT conversion path |
| `src/main.rs` | Default output to `.parquet` for SAS7BDAT input |

## Key Reference Files

| Purpose | Reference |
|---------|-----------|
| Constants & offsets | pandas `sas_constants.py` |
| Parser logic | pandas `sas7bdat.py` |
| RLE decompression | ReadStat `readstat_sas_rle.c` |
| RDC decompression | Parso `BinDecompressor.java` |
| Format specification | Shotwell `sas7bdat` repository |
| Encoding conversion | `encoding_rs` docs |

## Verification

```bash
# Build
cargo build

# Unit tests
cargo test --lib --all-features -- sas7bdat

# All tests (including integration)
cargo test --all-features

# Lint
cargo clippy --all-targets --all-features -- -D warnings
```
