# ADR-007: Dual CSV and Parquet Support

**Status:** Accepted
**Date:** 2026-02-01

---

## Context

Lo-phi must handle datasets from diverse sources. Data scientists and analysts typically work with CSV files due to universal tool support (Excel, Python pandas, R), human readability, and simple text-based editing. However, CSV has significant drawbacks: no schema enforcement (all columns parsed as strings without type inference), slow parsing (text-to-binary conversion), large file sizes (no compression), and no metadata storage (column types must be inferred).

Parquet addresses these limitations with columnar binary format, built-in compression (snappy/gzip), embedded schema metadata, and 5-10x faster read performance. However, Parquet requires specialized tools and is not human-readable. The tool must support both formats to serve different user workflows while encouraging migration to Parquet for performance-critical pipelines.

**Key Factors:**
- CSV ubiquity - most datasets arrive in CSV format from upstream systems
- Parquet performance - 5-10x faster loading and 3-5x smaller disk footprint
- User skill diversity - analysts comfortable with CSV, engineers prefer Parquet
- Workflow evolution - users start with CSV exploration, productionize with Parquet

## Decision

**Chosen Solution:** Support both CSV and Parquet for input/output, with built-in CSV-to-Parquet conversion accessible via TUI menu ([F] key) and CLI subcommand

Users can process CSV files directly for quick exploration, then convert to Parquet for production pipelines. The conversion utility preserves schema inference settings and offers streaming mode for large files.

## Alternatives Considered

### Alternative 1: CSV Only

**Description:** Support only CSV files, simplifying implementation and testing to a single code path.

**Pros:**
- Universal compatibility - every tool can read/write CSV
- Simplest implementation - no format detection logic needed
- Human-readable output for manual inspection and validation
- No learning curve - all users familiar with CSV

**Cons:**
- 5-10x slower performance on large datasets (10M+ rows)
- 3-5x larger file sizes consuming disk space and I/O bandwidth
- Schema inference failures on messy data (e.g., numeric columns with "N/A" text)
- No compression - datasets with repeated values waste space

**Rejection Reason:** Performance unacceptable for production credit risk workflows. 100M row datasets (common in consumer lending) would take 30+ minutes to load from CSV vs 3 minutes from Parquet.

---

### Alternative 2: Parquet Only

**Description:** Support only Parquet format, requiring users to convert CSV files externally before using Lo-phi.

**Pros:**
- Optimal performance - always use fastest format
- Smaller codebase - single I/O path to test
- Forces best practices - users learn Parquet early

**Cons:**
- Steep learning curve for CSV-native analysts
- Requires external tools (pandas, polars CLI, DuckDB) for conversion
- Breaks exploratory workflows where users want to quickly test on CSV samples
- Ecosystem friction - many upstream systems export only CSV

**Rejection Reason:** Accessibility barrier too high for target users (credit risk analysts, not data engineers). Forcing Parquet-only would limit adoption.

---

### Alternative 3: Format Auto-Detection with Silent Conversion

**Description:** Automatically detect CSV inputs and convert to temporary Parquet files transparently, cleaning up after execution.

**Pros:**
- Best of both worlds - users provide CSV, tool uses Parquet internally
- No user education needed - everything "just works"
- Optimal performance without user action

**Cons:**
- Hidden behavior confuses users when unexpected .parquet files appear in temp directories
- Conversion time not accounted for in progress bars - appears to "hang" during large CSV conversion
- Disk space consumption risk - 100GB CSV creates 30GB temp Parquet file without warning
- Cleanup failures leave temp files littering filesystem

**Rejection Reason:** Implicit behavior violates principle of least surprise. Users should explicitly request conversion and understand disk space requirements.

## Consequences

### Positive Outcomes

- **Flexibility:** Supports exploratory workflows (CSV) and production pipelines (Parquet) without tool switching.
- **Performance Path:** Users can measure CSV performance impact, then opt into Parquet conversion with one keystroke ([F] in TUI menu).
- **Ecosystem Compatibility:** CSV output compatible with Excel, Tableau, PowerBI for downstream reporting. Parquet output integrates with Spark, Snowflake, DuckDB for data engineering pipelines.

### Negative Outcomes / Trade-offs

- **Code Duplication:** Separate loading paths for CSV and Parquet add 200 lines of code and 2x testing surface area. Mitigated by shared interface abstracting format differences.
- **User Confusion:** Some users unaware of performance difference may stick with CSV unnecessarily. Addressed by TUI help text showing format comparison table.

### Neutral / Future Considerations

- **Additional Formats:** Future work could add Apache Arrow IPC or Apache Feather for zero-copy interoperability with Python processes. Low priority given Parquet already serves this use case.

## Implementation Notes

**Key Files:**
- `src/pipeline/loader.rs` - Lines 1-50: `get_column_names()` and `load_dataset_with_progress()` dispatch based on file extension
- `src/cli/convert.rs` - Full file: CSV-to-Parquet conversion with streaming and fast in-memory modes
- `src/cli/config_menu.rs` - [F] key handler triggers conversion workflow
- `src/main.rs` - Lines 740-772: `save_dataset()` writes CSV or Parquet based on output path extension

**Dependencies:**
- `polars = { version = "0.46", features = ["csv", "parquet", "streaming"] }` - Provides both format readers/writers

## References

- Parquet Format Documentation: https://parquet.apache.org/docs/
- Benchmarks: CSV vs Parquet performance comparison - https://www.robinlinacre.com/parquet_api/
