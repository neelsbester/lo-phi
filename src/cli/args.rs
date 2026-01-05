//! Command-line argument definitions using clap

use clap::Parser;
use std::path::PathBuf;

/// Lo-phi - Reduce dataset features using missing value and correlation analysis
#[derive(Parser, Debug)]
#[command(name = "lophi")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Input file path (CSV or Parquet)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Target column name (preserved during reduction).
    /// If not provided, will be selected interactively from available columns.
    #[arg(short, long)]
    pub target: Option<String>,

    /// Output file path (CSV or Parquet, determined by extension).
    /// Defaults to input directory with '_reduced' suffix (e.g., data.csv → data_reduced.csv)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Missing value threshold - drop features with missing values above this ratio
    #[arg(long, default_value = "0.3")]
    pub missing_threshold: f64,

    /// Correlation threshold - drop one feature from pairs with correlation above this value
    #[arg(long, default_value = "0.95")]
    pub correlation_threshold: f64,

    /// Gini threshold - drop features with Gini below this value (calculated via WoE binning)
    #[arg(long, default_value = "0.05")]
    pub gini_threshold: f64,

    /// Number of bins for Gini/IV calculation
    #[arg(long, default_value = "10")]
    pub gini_bins: usize,

    /// Skip interactive confirmation prompts
    #[arg(long, default_value = "false")]
    pub no_confirm: bool,

    /// Number of rows to use for schema inference (CSV only).
    /// Higher values improve type detection for ambiguous columns but may be slower.
    /// Use 0 for full table scan (very slow for large files).
    #[arg(long, default_value = "10000")]
    pub infer_schema_length: usize,
}

impl Args {
    /// Get the output path, deriving from input if not explicitly provided.
    /// The derived path will be in the same directory as the input with a '_reduced' suffix.
    pub fn output_path(&self) -> PathBuf {
        self.output.clone().unwrap_or_else(|| {
            let parent = self.input.parent().unwrap_or_else(|| std::path::Path::new("."));
            let stem = self.input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            let extension = self.input.extension().and_then(|e| e.to_str()).unwrap_or("parquet");
            parent.join(format!("{}_reduced.{}", stem, extension))
        })
    }
}

