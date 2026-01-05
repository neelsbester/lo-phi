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

/// Load and materialize dataset, returning collected DataFrame and stats
/// 
/// This function collects the lazy frame into memory and computes basic statistics.
/// Returns (DataFrame, rows, columns, memory_mb)
pub fn load_and_collect(lf: LazyFrame) -> Result<(DataFrame, usize, usize, f64)> {
    let df = lf.collect()?;
    let (rows, cols) = df.shape();
    let memory_mb = df.estimated_size() as f64 / (1024.0 * 1024.0);
    Ok((df, rows, cols, memory_mb))
}

