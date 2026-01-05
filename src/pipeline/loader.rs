//! Dataset loader for CSV and Parquet files

use anyhow::{Context, Result};
use polars::prelude::*;
use std::path::Path;

/// Load a dataset from a file (CSV or Parquet based on extension)
pub fn load_dataset(path: &Path) -> Result<LazyFrame> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let lf = match extension.as_str() {
        "csv" => LazyCsvReader::new(path)
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
    
    println!("\n📊 Dataset Statistics:");
    println!("   Rows: {}", rows);
    println!("   Columns: {}", cols);
    
    // Estimate memory usage
    let memory_bytes: usize = df.estimated_size();
    let memory_mb = memory_bytes as f64 / (1024.0 * 1024.0);
    println!("   Estimated memory: {:.2} MB", memory_mb);
    
    Ok(())
}

