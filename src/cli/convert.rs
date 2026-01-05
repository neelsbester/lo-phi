//! CSV to Parquet conversion utility

use std::path::Path;

use anyhow::{Context, Result};
use console::style;
use polars::prelude::*;

use crate::pipeline::load_dataset;
use crate::utils::create_spinner;

/// Run the CSV to Parquet conversion
///
/// # Arguments
/// * `input` - Path to the input CSV file
/// * `output` - Optional output path. If not provided, uses input path with .parquet extension
/// * `infer_schema_length` - Number of rows to use for schema inference
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

    // Load the CSV file
    let spinner = create_spinner("Loading CSV file...");
    let lf = load_dataset(input, infer_schema_length)?;
    let mut df = lf.collect()?;
    spinner.finish_with_message(format!("{} CSV loaded", style("✓").green()));

    // Get stats
    let (rows, cols) = df.shape();
    println!(
        "   {} rows × {} columns",
        style(rows).yellow(),
        style(cols).yellow()
    );

    // Write to Parquet
    let spinner = create_spinner("Writing Parquet file...");
    let file = std::fs::File::create(&output_path)
        .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;

    ParquetWriter::new(file)
        .finish(&mut df)
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

    println!();
    println!(
        "   {} File sizes:",
        style("✧").cyan()
    );
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

