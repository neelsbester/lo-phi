//! Bidirectional conversion utility between CSV, Parquet, and SAS7BDAT formats

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Local;
use console::style;
use polars::prelude::*;

use crate::pipeline::sas7bdat::load_sas7bdat;
use crate::utils::create_spinner;

/// Get current timestamp as HH:MM:SS
fn timestamp() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

/// Format duration in a human-readable way
fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs < 1.0 {
        format!("{:.0}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.2}s", secs)
    } else {
        let mins = (secs / 60.0).floor();
        let remaining_secs = secs - (mins * 60.0);
        format!("{:.0}m {:.1}s", mins, remaining_secs)
    }
}

/// Run file format conversion
///
/// # Arguments
/// * `input` - Path to the input file (CSV, Parquet, or SAS7BDAT)
/// * `output` - Optional output path. If not provided, auto-generates based on input format
/// * `infer_schema_length` - Number of rows to use for schema inference (CSV only)
/// * `fast` - If true, uses in-memory conversion (more RAM, all CPU cores).
///   If false, uses streaming conversion (low RAM, single-threaded).
///   Note: SAS7BDAT and Parquet always use in-memory mode.
///
/// # Supported Conversions
/// - CSV -> Parquet (streaming or fast mode)
/// - Parquet -> CSV (always in-memory)
/// - SAS7BDAT -> Parquet or CSV (always in-memory)
pub fn run_convert(
    input: &Path,
    output: Option<&Path>,
    infer_schema_length: usize,
    fast: bool,
) -> Result<()> {
    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match input_ext.as_str() {
        "sas7bdat" => return run_convert_sas7bdat(input, output),
        "parquet" => return run_convert_parquet(input, output),
        "csv" => {} // Fall through to existing CSV-to-Parquet logic below
        _ => anyhow::bail!(
            "Unsupported input format: .{}. Supported: .csv, .parquet, .sas7bdat",
            input_ext
        ),
    }

    let total_start = Instant::now();

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

    let mode_str = if fast {
        "fast (in-memory, multi-core)"
    } else {
        "streaming (low memory)"
    };

    println!(
        "\n {} Converting CSV to Parquet  {}",
        style("◆").cyan().bold(),
        style(format!("[started {}]", timestamp())).dim()
    );
    println!("   Input:  {}", style(input.display()).dim());
    println!("   Output: {}", style(output_path.display()).dim());
    println!(
        "   Schema inference: {} rows",
        style(if infer_schema_length == 0 {
            "full scan".to_string()
        } else {
            infer_schema_length.to_string()
        })
        .dim()
    );
    println!("   Mode: {}", style(mode_str).yellow());
    println!();

    // Convert schema length: 0 means full scan
    let schema_length = if infer_schema_length == 0 {
        None
    } else {
        Some(infer_schema_length)
    };

    // Variables to track timing
    let init_time;
    let schema_time;
    let load_time;
    let write_time;
    let num_cols;

    if fast {
        // === FAST MODE: In-memory conversion with parallelization ===

        // Step 1: Create LazyFrame from CSV
        println!(
            "   {} [{}] Initializing CSV reader...",
            style("→").blue(),
            style(timestamp()).dim()
        );
        let step_start = Instant::now();
        let spinner = create_spinner("Initializing CSV reader...");
        let lf = LazyCsvReader::new(input)
            .with_infer_schema_length(schema_length)
            .with_rechunk(true) // Rechunk for better parallel performance
            .finish()
            .with_context(|| format!("Failed to read CSV file: {}", input.display()))?;
        init_time = step_start.elapsed();
        spinner.finish_with_message(format!(
            "{} [{}] CSV reader initialized ({})",
            style("✓").green(),
            style(timestamp()).dim(),
            style(format_duration(init_time)).cyan()
        ));

        // Step 2: Collect schema
        println!(
            "   {} [{}] Inferring schema...",
            style("→").blue(),
            style(timestamp()).dim()
        );
        let step_start = Instant::now();
        let spinner = create_spinner("Inferring schema (reading sample rows)...");
        let schema = lf.clone().collect_schema()?;
        num_cols = schema.len();
        schema_time = step_start.elapsed();
        spinner.finish_with_message(format!(
            "{} [{}] Schema inferred: {} columns ({})",
            style("✓").green(),
            style(timestamp()).dim(),
            style(num_cols).yellow(),
            style(format_duration(schema_time)).cyan()
        ));

        // Step 3: Load entire dataset into memory (parallelized!)
        println!(
            "   {} [{}] Loading into memory (using all CPU cores)...",
            style("→").blue(),
            style(timestamp()).dim()
        );
        let step_start = Instant::now();
        let spinner = create_spinner("Loading dataset into memory...");
        let mut df = lf
            .collect()
            .with_context(|| "Failed to load dataset into memory")?;
        load_time = step_start.elapsed();
        spinner.finish_with_message(format!(
            "{} [{}] Loaded into memory ({})",
            style("✓").green(),
            style(timestamp()).dim(),
            style(format_duration(load_time)).cyan()
        ));

        // Step 4: Write to Parquet (parallelized column encoding!)
        println!(
            "   {} [{}] Writing Parquet (parallel column encoding)...",
            style("→").blue(),
            style(timestamp()).dim()
        );
        let step_start = Instant::now();
        let spinner = create_spinner("Writing Parquet file...");

        let file = std::fs::File::create(&output_path)
            .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;

        ParquetWriter::new(file)
            .with_compression(ParquetCompression::Snappy)
            .with_statistics(StatisticsOptions::full())
            .with_row_group_size(Some(100_000))
            .finish(&mut df)
            .with_context(|| format!("Failed to write Parquet file: {}", output_path.display()))?;

        write_time = step_start.elapsed();
        spinner.finish_with_message(format!(
            "{} [{}] Parquet written ({})",
            style("✓").green(),
            style(timestamp()).dim(),
            style(format_duration(write_time)).cyan()
        ));
    } else {
        // === STREAMING MODE: Memory-efficient but single-threaded ===
        load_time = std::time::Duration::ZERO; // No separate load step in streaming mode

        // Step 1: Create LazyFrame from CSV
        println!(
            "   {} [{}] Initializing CSV reader...",
            style("→").blue(),
            style(timestamp()).dim()
        );
        let step_start = Instant::now();
        let spinner = create_spinner("Initializing CSV reader...");
        let lf = LazyCsvReader::new(input)
            .with_infer_schema_length(schema_length)
            .with_low_memory(true) // Reduces memory pressure for large files
            .with_rechunk(false) // No rechunking needed for streaming
            .finish()
            .with_context(|| format!("Failed to read CSV file: {}", input.display()))?;
        init_time = step_start.elapsed();
        spinner.finish_with_message(format!(
            "{} [{}] CSV reader initialized ({})",
            style("✓").green(),
            style(timestamp()).dim(),
            style(format_duration(init_time)).cyan()
        ));

        // Step 2: Collect schema (triggers schema inference)
        println!(
            "   {} [{}] Inferring schema...",
            style("→").blue(),
            style(timestamp()).dim()
        );
        let step_start = Instant::now();
        let spinner = create_spinner("Inferring schema (reading sample rows)...");
        let schema = lf.clone().collect_schema()?;
        num_cols = schema.len();
        schema_time = step_start.elapsed();
        spinner.finish_with_message(format!(
            "{} [{}] Schema inferred: {} columns ({})",
            style("✓").green(),
            style(timestamp()).dim(),
            style(num_cols).yellow(),
            style(format_duration(schema_time)).cyan()
        ));

        // Step 3: Stream directly to Parquet without collecting into memory
        println!(
            "   {} [{}] Streaming to Parquet (single-threaded)...",
            style("→").blue(),
            style(timestamp()).dim()
        );
        let step_start = Instant::now();
        let spinner =
            create_spinner("Streaming to Parquet (this may take a while for large files)...");

        // Configure Parquet write options for optimal performance
        let parquet_options = ParquetWriteOptions {
            compression: ParquetCompression::Snappy,
            statistics: StatisticsOptions::full(),
            row_group_size: Some(100_000),
            ..Default::default()
        };

        lf.sink_parquet(&output_path, parquet_options, None)
            .with_context(|| format!("Failed to write Parquet file: {}", output_path.display()))?;

        write_time = step_start.elapsed();
        spinner.finish_with_message(format!(
            "{} [{}] Parquet written ({})",
            style("✓").green(),
            style(timestamp()).dim(),
            style(format_duration(write_time)).cyan()
        ));
    }

    // Show file size comparison
    let input_size_bytes = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let output_size_bytes = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let input_size = input_size_bytes as f64 / (1024.0 * 1024.0);
    let output_size = output_size_bytes as f64 / (1024.0 * 1024.0);

    // Get row count from the output file (Parquet metadata is fast to read)
    let row_count = get_parquet_row_count(&output_path).unwrap_or(0);

    let total_time = total_start.elapsed();
    let throughput_mb_s = input_size / total_time.as_secs_f64();

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
    println!("   {} Timing breakdown:", style("⏱").cyan());
    println!("      Init:    {}", format_duration(init_time));
    println!("      Schema:  {}", format_duration(schema_time));
    if fast {
        println!("      Load:    {}", format_duration(load_time));
    }
    println!("      Write:   {}", format_duration(write_time));
    println!(
        "      {}",
        style(format!("Total:   {}", format_duration(total_time))).bold()
    );
    println!("      Throughput: {:.1} MB/s", throughput_mb_s);

    println!();
    println!(" {} Conversion complete!", style("✓").green().bold());

    Ok(())
}

/// Run Parquet to CSV conversion
///
/// Loads the Parquet file and writes to CSV.
fn run_convert_parquet(input: &Path, output: Option<&Path>) -> Result<()> {
    let total_start = Instant::now();

    // Determine output path
    let output_path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let parent = input.parent().unwrap_or_else(|| Path::new("."));
            let stem = input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            parent.join(format!("{}.csv", stem))
        }
    };

    println!(
        "\n {} Converting Parquet to CSV  {}",
        style("◆").cyan().bold(),
        style(format!("[started {}]", timestamp())).dim()
    );
    println!("   Input:  {}", style(input.display()).dim());
    println!("   Output: {}", style(output_path.display()).dim());
    println!();

    // Step 1: Load Parquet file
    println!(
        "   {} [{}] Loading Parquet file...",
        style("→").blue(),
        style(timestamp()).dim()
    );
    let step_start = Instant::now();
    let spinner = create_spinner("Loading Parquet file...");
    let lf = LazyFrame::scan_parquet(input, Default::default())
        .with_context(|| format!("Failed to read Parquet file: {}", input.display()))?;
    let schema = lf.clone().collect_schema()?;
    let num_cols = schema.len();
    let init_time = step_start.elapsed();
    spinner.finish_with_message(format!(
        "{} [{}] Parquet reader initialized: {} columns ({})",
        style("✓").green(),
        style(timestamp()).dim(),
        style(num_cols).yellow(),
        style(format_duration(init_time)).cyan()
    ));

    // Step 2: Collect into memory
    println!(
        "   {} [{}] Loading into memory...",
        style("→").blue(),
        style(timestamp()).dim()
    );
    let step_start = Instant::now();
    let spinner = create_spinner("Loading dataset into memory...");
    let mut df = lf
        .collect()
        .with_context(|| "Failed to load Parquet into memory")?;
    let row_count = df.height();
    let load_time = step_start.elapsed();
    spinner.finish_with_message(format!(
        "{} [{}] Loaded: {} rows ({})",
        style("✓").green(),
        style(timestamp()).dim(),
        style(row_count).yellow(),
        style(format_duration(load_time)).cyan()
    ));

    // Step 3: Write CSV
    println!(
        "   {} [{}] Writing CSV...",
        style("→").blue(),
        style(timestamp()).dim()
    );
    let step_start = Instant::now();
    let spinner = create_spinner("Writing CSV file...");
    let file = std::fs::File::create(&output_path)
        .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;
    CsvWriter::new(file)
        .finish(&mut df)
        .with_context(|| format!("Failed to write CSV file: {}", output_path.display()))?;
    let write_time = step_start.elapsed();
    spinner.finish_with_message(format!(
        "{} [{}] CSV written ({})",
        style("✓").green(),
        style(timestamp()).dim(),
        style(format_duration(write_time)).cyan()
    ));

    // Show file size comparison
    let input_size_bytes = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let output_size_bytes = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let input_size = input_size_bytes as f64 / (1024.0 * 1024.0);
    let output_size = output_size_bytes as f64 / (1024.0 * 1024.0);
    let total_time = total_start.elapsed();
    let throughput_mb_s = input_size / total_time.as_secs_f64();

    println!();
    println!(
        "   {} rows x {} columns",
        style(row_count).yellow(),
        style(num_cols).yellow()
    );
    println!("   {} File sizes:", style("✧").cyan());
    println!("      Parquet: {:.2} MB", input_size);
    println!("      CSV:     {:.2} MB", output_size);

    if output_size < input_size {
        let reduction = ((input_size - output_size) / input_size) * 100.0;
        println!(
            "      {}",
            style(format!("↓ {:.1}% smaller", reduction)).green()
        );
    }

    println!();
    println!("   {} Timing breakdown:", style("⏱").cyan());
    println!("      Init:    {}", format_duration(init_time));
    println!("      Load:    {}", format_duration(load_time));
    println!("      Write:   {}", format_duration(write_time));
    println!(
        "      {}",
        style(format!("Total:   {}", format_duration(total_time))).bold()
    );
    println!("      Throughput: {:.1} MB/s", throughput_mb_s);

    println!();
    println!(" {} Conversion complete!", style("✓").green().bold());

    Ok(())
}

/// Run SAS7BDAT to Parquet/CSV conversion
///
/// Loads the SAS7BDAT file using the pure Rust parser, then writes to
/// Parquet (default) or CSV (if output path has .csv extension).
fn run_convert_sas7bdat(input: &Path, output: Option<&Path>) -> Result<()> {
    let total_start = Instant::now();

    // Determine output path and format
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

    let output_ext = output_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("parquet")
        .to_lowercase();

    let format_name = if output_ext == "csv" {
        "CSV"
    } else {
        "Parquet"
    };

    println!(
        "\n {} Converting SAS7BDAT to {}  {}",
        style("◆").cyan().bold(),
        format_name,
        style(format!("[started {}]", timestamp())).dim()
    );
    println!("   Input:  {}", style(input.display()).dim());
    println!("   Output: {}", style(output_path.display()).dim());
    println!();

    // Step 1: Load SAS7BDAT file
    let spinner = create_spinner("Loading SAS7BDAT file...");
    let (mut df, rows, cols, _) =
        load_sas7bdat(input).map_err(|e| anyhow::anyhow!("Failed to load SAS7BDAT file: {}", e))?;
    let load_time = total_start.elapsed();
    spinner.finish_with_message(format!(
        "{} [{}] SAS7BDAT loaded: {} rows x {} columns ({})",
        style("✓").green(),
        style(timestamp()).dim(),
        style(rows).yellow(),
        style(cols).yellow(),
        style(format_duration(load_time)).cyan()
    ));

    // Step 2: Write output
    let write_start = Instant::now();
    let msg = format!("Writing {} file...", format_name);
    let spinner = create_spinner(&msg);

    if output_ext == "csv" {
        let file = std::fs::File::create(&output_path)
            .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;
        CsvWriter::new(file)
            .finish(&mut df)
            .with_context(|| format!("Failed to write CSV file: {}", output_path.display()))?;
    } else {
        let file = std::fs::File::create(&output_path)
            .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;
        ParquetWriter::new(file)
            .with_compression(ParquetCompression::Snappy)
            .with_statistics(StatisticsOptions::full())
            .with_row_group_size(Some(100_000))
            .finish(&mut df)
            .with_context(|| format!("Failed to write Parquet file: {}", output_path.display()))?;
    }

    let write_time = write_start.elapsed();
    spinner.finish_with_message(format!(
        "{} [{}] {} written ({})",
        style("✓").green(),
        style(timestamp()).dim(),
        format_name,
        style(format_duration(write_time)).cyan()
    ));

    // Show file sizes
    let input_size_bytes = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let output_size_bytes = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let input_size = input_size_bytes as f64 / (1024.0 * 1024.0);
    let output_size = output_size_bytes as f64 / (1024.0 * 1024.0);
    let total_time = total_start.elapsed();

    println!();
    println!(
        "   {} rows x {} columns",
        style(rows).yellow(),
        style(cols).yellow()
    );
    println!("   {} File sizes:", style("✧").cyan());
    println!("      SAS7BDAT: {:.2} MB", input_size);
    println!("      {}:  {:.2} MB", format_name, output_size);

    if output_size < input_size {
        let reduction = ((input_size - output_size) / input_size) * 100.0;
        println!(
            "      {}",
            style(format!("↓ {:.1}% smaller", reduction)).green()
        );
    }

    println!();
    println!(
        "   {}",
        style(format!("Total: {}", format_duration(total_time))).bold()
    );
    println!();
    println!(" {} Conversion complete!", style("✓").green().bold());

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
