//! Dataset loader for CSV and Parquet files

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use polars::prelude::*;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
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

/// Load a CSV file with a progress bar showing bytes read
fn load_csv_with_progress(path: &Path, schema_length: Option<usize>) -> Result<DataFrame> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open CSV file: {}", path.display()))?;
    let file_size = file
        .metadata()
        .with_context(|| "Failed to get file metadata")?
        .len();

    // Create progress bar with byte-tracking style
    let pb = ProgressBar::new(file_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "   Loading CSV [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%) [{eta}]",
            )
            .unwrap()
            .progress_chars("=>-"),
    );

    // Read file with progress tracking
    let mut reader = BufReader::with_capacity(1024 * 1024, file); // 1MB buffer
    let mut buffer = Vec::with_capacity(file_size as usize);
    let mut chunk = [0u8; 65536]; // 64KB read chunks

    loop {
        let bytes_read = reader.read(&mut chunk)?;
        if bytes_read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes_read]);
        pb.inc(bytes_read as u64);
    }

    pb.finish_and_clear();

    // Show spinner during parsing phase
    let parse_spinner = ProgressBar::new_spinner();
    parse_spinner.set_style(
        ProgressStyle::default_spinner()
            .template("   {spinner:.cyan} Converting and calculating summary statistics...")
            .unwrap(),
    );
    parse_spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    // Parse the buffered data using CsvReadOptions for proper schema inference
    let cursor = Cursor::new(buffer);
    let df = CsvReadOptions::default()
        .with_infer_schema_length(schema_length) // Use user's schema inference setting (None = full scan)
        .with_rechunk(true) // Consolidate chunks for better downstream performance
        .into_reader_with_file_handle(cursor)
        .finish()
        .with_context(|| format!("Failed to parse CSV file: {}", path.display()))?;

    parse_spinner.finish_and_clear();

    Ok(df)
}

/// Load a Parquet file (uses lazy scanning which is already fast)
fn load_parquet(path: &Path) -> Result<DataFrame> {
    // Enable parallel row group reading for multi-core I/O
    let args = ScanArgsParquet {
        parallel: ParallelStrategy::Auto,
        ..Default::default()
    };

    LazyFrame::scan_parquet(path, args)
        .with_context(|| format!("Failed to scan Parquet file: {}", path.display()))?
        .collect()
        .with_context(|| format!("Failed to collect Parquet file: {}", path.display()))
}

/// Load a dataset from a file (CSV or Parquet based on extension)
///
/// # Arguments
/// * `path` - Path to the input file
/// * `infer_schema_length` - Number of rows to use for schema inference (CSV only).
///   A value of 0 means full table scan. Default in Polars is 100, but higher
///   values help with ambiguous column types.
///
/// # Returns
/// LazyFrame for further processing
///
/// # Performance Notes
/// - **Parquet**: Uses parallel row group reading for multi-core I/O
/// - **CSV**: I/O is inherently sequential due to format limitations, but parsing
///   is parallelized. For large datasets, prefer Parquet format.
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
            .with_low_memory(true) // Reduces memory pressure for large files
            .with_rechunk(true) // Consolidates chunks for better performance
            .finish()
            .with_context(|| format!("Failed to load CSV file: {}", path.display()))?,
        "parquet" => {
            // Enable parallel row group reading for multi-core I/O
            let args = ScanArgsParquet {
                parallel: ParallelStrategy::Auto,
                ..Default::default()
            };
            LazyFrame::scan_parquet(path, args)
                .with_context(|| format!("Failed to load Parquet file: {}", path.display()))?
        }
        _ => anyhow::bail!(
            "Unsupported file format: {}. Supported formats: csv, parquet",
            extension
        ),
    };

    Ok(lf)
}

/// Load dataset with progress bar and return DataFrame with statistics
///
/// This is the preferred method for loading datasets as it:
/// - Shows a progress bar for CSV files (based on bytes read)
/// - Uses efficient parallel loading for Parquet files
/// - Returns the DataFrame directly along with statistics
///
/// # Arguments
/// * `path` - Path to the input file
/// * `infer_schema_length` - Number of rows to use for schema inference (CSV only)
///
/// # Returns
/// Tuple of (DataFrame, rows, columns, memory_mb)
pub fn load_dataset_with_progress(
    path: &Path,
    infer_schema_length: usize,
) -> Result<(DataFrame, usize, usize, f64)> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let schema_length = if infer_schema_length == 0 {
        None
    } else {
        Some(infer_schema_length)
    };

    let df = match extension.as_str() {
        "csv" => load_csv_with_progress(path, schema_length)?,
        "parquet" => load_parquet(path)?,
        _ => anyhow::bail!(
            "Unsupported file format: {}. Supported formats: csv, parquet",
            extension
        ),
    };

    let (rows, cols) = df.shape();
    let memory_mb = df.estimated_size() as f64 / (1024.0 * 1024.0);

    Ok((df, rows, cols, memory_mb))
}

