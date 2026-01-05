//! CSV to Parquet conversion utility with streaming support

use std::path::Path;

use anyhow::{Context, Result};
use console::style;
use polars::prelude::*;

use crate::utils::create_spinner;

/// Run the CSV to Parquet conversion using streaming for memory efficiency
///
/// # Arguments
/// * `input` - Path to the input CSV file
/// * `output` - Optional output path. If not provided, uses input path with .parquet extension
/// * `infer_schema_length` - Number of rows to use for schema inference
///
/// # Performance Notes
/// Uses streaming `sink_parquet()` to convert without loading the entire dataset into memory.
/// This is significantly faster and more memory-efficient for large files.
pub fn run_convert(input: &Path, output: Option<&Path>, infer_schema_length: usize) -> Result<()> {
    // Determine output path
    let output_path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let parent = input.parent().unwrap_or_else(|| Path::new("."));
            let stem = input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            parent.join(format!("{}.parquet", stem))
        }
    };

    println!(
        "\n {} Converting CSV to Parquet",
        style("◆").cyan().bold()
    );
    println!("   Input:  {}", style(input.display()).dim());
    println!("   Output: {}", style(output_path.display()).dim());
    println!();

    // Convert schema length: 0 means full scan
    let schema_length = if infer_schema_length == 0 {
        None
    } else {
        Some(infer_schema_length)
    };

    // Create LazyFrame from CSV
    let spinner = create_spinner("Reading CSV schema...");
    let lf = LazyCsvReader::new(input)
        .with_infer_schema_length(schema_length)
        .with_rechunk(false) // No rechunking needed for streaming
        .finish()
        .with_context(|| format!("Failed to read CSV file: {}", input.display()))?;

    // Get column count from schema (cheap metadata operation)
    let schema = lf.clone().collect_schema()?;
    let num_cols = schema.len();
    spinner.finish_with_message(format!("{} Schema loaded ({} columns)", style("✓").green(), num_cols));

    // Stream directly to Parquet without collecting into memory
    let spinner = create_spinner("Streaming to Parquet...");

    // Configure Parquet write options for optimal performance
    let parquet_options = ParquetWriteOptions {
        compression: ParquetCompression::Snappy,
        statistics: StatisticsOptions::full(),
        row_group_size: Some(100_000), // Optimal row group size for query performance
        ..Default::default()
    };

    lf.sink_parquet(&output_path, parquet_options, None)
        .with_context(|| format!("Failed to write Parquet file: {}", output_path.display()))?;

    spinner.finish_with_message(format!("{} Parquet written", style("✓").green()));

    // Show file size comparison
    let input_size = std::fs::metadata(input)
        .map(|m| m.len())
        .unwrap_or(0) as f64
        / (1024.0 * 1024.0);
    let output_size = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0) as f64
        / (1024.0 * 1024.0);

    // Get row count from the output file (Parquet metadata is fast to read)
    let row_count = get_parquet_row_count(&output_path).unwrap_or(0);

    println!();
    println!(
        "   {} rows × {} columns",
        style(row_count).yellow(),
        style(num_cols).yellow()
    );
    println!("   {} File sizes:", style("✧").cyan());
    println!("      CSV:     {:.2} MB", input_size);
    println!("      Parquet: {:.2} MB", output_size);

    if output_size < input_size {
        let reduction = ((input_size - output_size) / input_size) * 100.0;
        println!(
            "      {}",
            style(format!("↓ {:.1}% smaller", reduction)).green()
        );
    }

    println!();
    println!(
        " {} Conversion complete!",
        style("✓").green().bold()
    );

    Ok(())
}

/// Get row count from a Parquet file using metadata (fast, no full scan)
fn get_parquet_row_count(path: &Path) -> Result<usize> {
    let lf = LazyFrame::scan_parquet(path, Default::default())?;
    let df = lf.select([len()]).collect()?;
    let count = df.column("len")?.get(0)?;
    match count {
        AnyValue::UInt32(n) => Ok(n as usize),
        AnyValue::UInt64(n) => Ok(n as usize),
        AnyValue::Int32(n) => Ok(n as usize),
        AnyValue::Int64(n) => Ok(n as usize),
        _ => Ok(0),
    }
}
