//! Dataset loader for CSV and Parquet files

use anyhow::{Context, Result};
use polars::prelude::*;
use std::path::Path;

/// Get column names from a dataset file without loading all data.
/// Useful for interactive column selection.
pub fn get_column_names(path: &Path) -> Result<Vec<String>> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let schema = match extension.as_str() {
        "csv" => {
            let mut lf = LazyCsvReader::new(path)
                .with_infer_schema_length(Some(100))
                .finish()
                .with_context(|| format!("Failed to read CSV schema: {}", path.display()))?;
            lf.collect_schema()?
        }
        "parquet" => {
            let mut lf = LazyFrame::scan_parquet(path, Default::default())
                .with_context(|| format!("Failed to read Parquet schema: {}", path.display()))?;
            lf.collect_schema()?
        }
        _ => anyhow::bail!(
            "Unsupported file format: {}. Supported formats: csv, parquet",
            extension
        ),
    };

    Ok(schema.iter_names().map(|s| s.to_string()).collect())
}

/// Load a dataset from a file (CSV or Parquet based on extension)
///
/// # Arguments
/// * `path` - Path to the input file
/// * `infer_schema_length` - Number of rows to use for schema inference (CSV only).
///   A value of 0 means full table scan. Default in Polars is 100, but higher
///   values help with ambiguous column types.
pub fn load_dataset(path: &Path, infer_schema_length: usize) -> Result<LazyFrame> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Convert 0 to None for full table scan, otherwise Some(n)
    let schema_length = if infer_schema_length == 0 {
        None
    } else {
        Some(infer_schema_length)
    };

    let lf = match extension.as_str() {
        "csv" => LazyCsvReader::new(path)
            .with_infer_schema_length(schema_length)
            .finish()
            .with_context(|| format!("Failed to load CSV file: {}", path.display()))?,
        "parquet" => LazyFrame::scan_parquet(path, Default::default())
            .with_context(|| format!("Failed to load Parquet file: {}", path.display()))?,
        _ => anyhow::bail!(
            "Unsupported file format: {}. Supported formats: csv, parquet",
            extension
        ),
    };

    Ok(lf)
}

/// Display initial statistics about the dataset
pub fn display_dataset_stats(lf: &LazyFrame) -> Result<()> {
    let df = lf.clone().collect()?;
    let (rows, cols) = df.shape();
    
    println!("\n    ✧ Dataset Statistics:");
    println!("   Rows: {}", rows);
    println!("   Columns: {}", cols);
    
    // Estimate memory usage
    let memory_bytes: usize = df.estimated_size();
    let memory_mb = memory_bytes as f64 / (1024.0 * 1024.0);
    println!("   Estimated memory: {:.2} MB", memory_mb);
    
    Ok(())
}

