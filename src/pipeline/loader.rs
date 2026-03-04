//! Dataset loader for CSV and Parquet files

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use polars::prelude::*;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::Path;

use super::progress::{PipelineStage, ProgressEvent, ProgressSender};

/// Get column names from a dataset file without loading all data.
/// Useful for interactive column selection.
pub fn get_column_names(path: &Path) -> Result<Vec<String>> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "csv" => {
            let mut lf = LazyCsvReader::new(path)
                .with_infer_schema_length(Some(100))
                .finish()
                .with_context(|| format!("Failed to read CSV schema: {}", path.display()))?;
            let schema = lf.collect_schema()?;
            Ok(schema.iter_names().map(|s| s.to_string()).collect())
        }
        "parquet" => {
            let mut lf = LazyFrame::scan_parquet(path, Default::default())
                .with_context(|| format!("Failed to read Parquet schema: {}", path.display()))?;
            let schema = lf.collect_schema()?;
            Ok(schema.iter_names().map(|s| s.to_string()).collect())
        }
        "sas7bdat" => {
            use super::sas7bdat::get_sas7bdat_columns;
            get_sas7bdat_columns(path).context("Failed to read SAS7BDAT columns")
        }
        _ => anyhow::bail!(
            "Unsupported file format: {}. Supported formats: csv, parquet, sas7bdat",
            extension
        ),
    }
}

/// Load a CSV file with a progress bar showing bytes read.
/// When `progress_tx` is `Some`, sends `ProgressEvent::update` messages instead of
/// writing to an indicatif bar.
fn load_csv_with_progress_inner(
    path: &Path,
    schema_length: Option<usize>,
    progress_tx: Option<&ProgressSender>,
) -> Result<DataFrame> {
    let file =
        File::open(path).with_context(|| format!("Failed to open CSV file: {}", path.display()))?;
    let file_size = file
        .metadata()
        .with_context(|| "Failed to get file metadata")?
        .len();

    // Read file with optional indicatif bar or channel updates
    let mut reader = BufReader::with_capacity(1024 * 1024, file); // 1MB buffer
    let mut buffer = Vec::with_capacity(file_size as usize);
    let mut chunk = [0u8; 65536]; // 64KB read chunks

    // Only create indicatif bar in terminal-output mode
    let pb = if progress_tx.is_none() {
        let bar = ProgressBar::new(file_size);
        bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "   Loading CSV [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%) [{eta}]",
                )
                .unwrap()
                .progress_chars("=>-"),
        );
        Some(bar)
    } else {
        None
    };

    let mut bytes_read_total: u64 = 0;
    let update_interval = (file_size / 20).max(65536); // ~5% intervals

    loop {
        let bytes_read = reader.read(&mut chunk)?;
        if bytes_read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes_read]);
        bytes_read_total += bytes_read as u64;

        if let Some(bar) = &pb {
            bar.inc(bytes_read as u64);
        } else if let Some(tx) = progress_tx {
            // Throttle channel updates to avoid flooding
            if bytes_read_total % update_interval < bytes_read as u64 {
                let pct = bytes_read_total * 100 / file_size.max(1);
                tx.send(ProgressEvent::update(
                    PipelineStage::Loading,
                    "Loading dataset",
                    format!("{}% read", pct),
                ))
                .ok();
            }
        }
    }

    if let Some(bar) = &pb {
        bar.finish_and_clear();
    }

    // Parse phase
    if let Some(tx) = progress_tx {
        tx.send(ProgressEvent::update(
            PipelineStage::Loading,
            "Loading dataset",
            "Parsing CSV…",
        ))
        .ok();
    } else {
        let parse_spinner = ProgressBar::new_spinner();
        parse_spinner.set_style(
            ProgressStyle::default_spinner()
                .template("   {spinner:.cyan} Converting and calculating summary statistics...")
                .unwrap(),
        );
        parse_spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let cursor = Cursor::new(buffer);
        let df = CsvReadOptions::default()
            .with_infer_schema_length(schema_length)
            .with_rechunk(true)
            .into_reader_with_file_handle(cursor)
            .finish()
            .with_context(|| format!("Failed to parse CSV file: {}", path.display()))?;

        parse_spinner.finish_and_clear();
        return Ok(df);
    }

    let cursor = Cursor::new(buffer);
    let df = CsvReadOptions::default()
        .with_infer_schema_length(schema_length)
        .with_rechunk(true)
        .into_reader_with_file_handle(cursor)
        .finish()
        .with_context(|| format!("Failed to parse CSV file: {}", path.display()))?;

    Ok(df)
}

/// Load a CSV file with a progress bar showing bytes read (terminal / indicatif path).
fn load_csv_with_progress(path: &Path, schema_length: Option<usize>) -> Result<DataFrame> {
    load_csv_with_progress_inner(path, schema_length, None)
}

/// Load a Parquet file (uses lazy scanning which is already fast)
fn load_parquet(path: &Path) -> Result<DataFrame> {
    // Enable parallel row group reading for multi-core I/O
    let args = ScanArgsParquet {
        parallel: ParallelStrategy::Auto,
        ..Default::default()
    };

    let mut df = LazyFrame::scan_parquet(path, args)
        .with_context(|| format!("Failed to scan Parquet file: {}", path.display()))?
        .collect()
        .with_context(|| format!("Failed to collect Parquet file: {}", path.display()))?;

    // Rechunk to consolidate row groups into a single contiguous chunk.
    // This ensures consistent iteration when zipping with weight vectors downstream.
    // Without this, Parquet files with multiple row groups can cause chunk mismatch panics.
    df.rechunk_mut();
    Ok(df)
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
    load_dataset_impl(path, infer_schema_length, None)
}

/// Load dataset and optionally send progress events over a channel instead of
/// rendering indicatif bars to the terminal.
pub fn load_dataset_with_progress_channel(
    path: &Path,
    infer_schema_length: usize,
    progress_tx: &ProgressSender,
) -> Result<(DataFrame, usize, usize, f64)> {
    load_dataset_impl(path, infer_schema_length, Some(progress_tx))
}

fn load_dataset_impl(
    path: &Path,
    infer_schema_length: usize,
    progress_tx: Option<&ProgressSender>,
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
        "csv" => {
            if let Some(tx) = progress_tx {
                load_csv_with_progress_inner(path, schema_length, Some(tx))?
            } else {
                load_csv_with_progress(path, schema_length)?
            }
        }
        "parquet" => {
            if let Some(tx) = progress_tx {
                tx.send(ProgressEvent::update(
                    PipelineStage::Loading,
                    "Loading dataset",
                    "Reading Parquet file…",
                ))
                .ok();
            }
            load_parquet(path)?
        }
        "sas7bdat" => {
            // NOTE: schema_length is unused for SAS7BDAT files because column types are
            // encoded explicitly in the binary header (no schema inference needed).
            let silent = if let Some(tx) = progress_tx {
                tx.send(ProgressEvent::update(
                    PipelineStage::Loading,
                    "Loading dataset",
                    "Reading SAS7BDAT file…",
                ))
                .ok();
                true
            } else {
                false
            };
            if silent {
                use super::sas7bdat::load_sas7bdat_silent;
                let (mut df, _, _, _) =
                    load_sas7bdat_silent(path).context("Failed to load SAS7BDAT file")?;
                df.rechunk_mut();
                df
            } else {
                use super::sas7bdat::load_sas7bdat;
                let (mut df, _, _, _) =
                    load_sas7bdat(path).context("Failed to load SAS7BDAT file")?;
                df.rechunk_mut();
                df
            }
        }
        _ => anyhow::bail!(
            "Unsupported file format: {}. Supported formats: csv, parquet, sas7bdat",
            extension
        ),
    };

    let (rows, cols) = df.shape();
    let memory_mb = df.estimated_size() as f64 / (1024.0 * 1024.0);

    Ok((df, rows, cols, memory_mb))
}
