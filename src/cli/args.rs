//! Command-line argument definitions using clap

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Lo-phi - Reduce dataset features using missing value and correlation analysis
#[derive(Parser, Debug)]
#[command(name = "lophi")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Input file path (CSV or Parquet)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Target column name (preserved during reduction).
    /// If not provided, will be selected interactively from available columns.
    #[arg(short, long)]
    pub target: Option<String>,

    /// Value in target column that represents EVENT (maps to 1).
    /// Required with --non-event-value when target is not binary 0/1.
    #[arg(long)]
    pub event_value: Option<String>,

    /// Value in target column that represents NON-EVENT (maps to 0).
    /// Required with --event-value when target is not binary 0/1.
    #[arg(long)]
    pub non_event_value: Option<String>,

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

    /// Binning strategy for Gini/IV calculation.
    /// Options: "quantile" (equal-frequency, default) or "cart" (decision tree splits)
    #[arg(long, default_value = "quantile")]
    pub binning_strategy: String,

    /// Minimum samples per category for categorical features.
    /// Categories with fewer samples are merged into "OTHER".
    #[arg(long, default_value = "5")]
    pub min_category_samples: usize,

    /// Columns to drop before processing (comma-separated).
    /// These columns will be removed from the dataset before any analysis.
    #[arg(long, value_delimiter = ',')]
    pub drop_columns: Vec<String>,

    /// Skip interactive confirmation prompts
    #[arg(long, default_value = "false")]
    pub no_confirm: bool,

    /// Number of rows to use for schema inference (CSV only).
    /// Higher values improve type detection for ambiguous columns but may be slower.
    /// Use 0 for full table scan (very slow for large files).
    #[arg(long, default_value = "10000")]
    pub infer_schema_length: usize,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Convert CSV file to Parquet format
    Convert {
        /// Input CSV file path
        input: PathBuf,

        /// Output Parquet file path (optional, defaults to input with .parquet extension)
        output: Option<PathBuf>,

        /// Number of rows to use for schema inference.
        /// Higher values improve type detection for ambiguous columns but may be slower.
        /// Use 0 for full table scan (very slow for large files).
        #[arg(long, default_value = "10000")]
        infer_schema_length: usize,

        /// Use fast in-memory conversion (uses more RAM but parallelizes across all CPU cores).
        /// Recommended for machines with sufficient RAM (roughly 2-3x the CSV file size).
        /// Without this flag, uses memory-efficient streaming (single-threaded but low RAM).
        #[arg(long, default_value = "false")]
        fast: bool,
    },
}

impl Cli {
    /// Get the input path, returning an error if not provided when running the reduce pipeline.
    pub fn input(&self) -> Option<&PathBuf> {
        self.input.as_ref()
    }

    /// Get the output path, deriving from input if not explicitly provided.
    /// The derived path will be in the same directory as the input with a '_reduced' suffix.
    pub fn output_path(&self) -> Option<PathBuf> {
        let input = self.input.as_ref()?;
        Some(self.output.clone().unwrap_or_else(|| {
            let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
            let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            let extension = input.extension().and_then(|e| e.to_str()).unwrap_or("parquet");
            parent.join(format!("{}_reduced.{}", stem, extension))
        }))
    }

    /// Get the Gini analysis output path, derived from the input file.
    /// The derived path will be in the same directory as the input with a '_gini_analysis.json' suffix.
    pub fn gini_analysis_path(&self) -> Option<PathBuf> {
        let input = self.input.as_ref()?;
        let parent = input.parent().unwrap_or_else(|| std::path::Path::new("."));
        let stem = input.file_stem().and_then(|s| s.to_str())?;
        Some(parent.join(format!("{}_gini_analysis.json", stem)))
    }
}

