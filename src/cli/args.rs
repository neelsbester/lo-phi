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

    /// Input file path (CSV, Parquet, or SAS7BDAT)
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

    /// Column containing sample weights for weighted analysis.
    /// When specified, all calculations (missing ratio, IV/Gini, correlation)
    /// use weighted statistics. Default: equal weights of 1.0 for all rows.
    #[arg(short = 'w', long)]
    pub weight_column: Option<String>,

    /// Output file path (CSV or Parquet, determined by extension).
    /// Defaults to input directory with '_reduced' suffix (e.g., data.csv → data_reduced.csv).
    /// SAS7BDAT input defaults to Parquet output.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Missing value threshold - drop features with missing values above this ratio
    #[arg(long, default_value = "0.3")]
    pub missing_threshold: f64,

    /// Correlation threshold - drop one feature from pairs with correlation above this value
    #[arg(long, default_value = "0.40")]
    pub correlation_threshold: f64,

    /// Gini threshold - drop features with Gini below this value (calculated via WoE binning)
    #[arg(long, default_value = "0.05")]
    pub gini_threshold: f64,

    /// Number of bins for Gini/IV calculation
    #[arg(long, default_value = "10")]
    pub gini_bins: usize,

    /// Binning strategy for Gini/IV calculation.
    /// Options: "cart" (decision tree splits, default) or "quantile" (equal-frequency)
    #[arg(long, default_value = "cart")]
    pub binning_strategy: String,

    /// Number of prebins for initial binning before optimization/merging.
    /// Lower values = faster but less granular. Higher values = more precise but slower solver.
    #[arg(long, default_value = "20")]
    pub prebins: usize,

    /// Enable solver-based optimal binning (MIP optimization).
    /// When enabled, uses mathematical optimization instead of greedy merging.
    /// Slower but produces globally optimal bin boundaries with optional monotonicity constraints.
    #[arg(long, default_value = "true")]
    pub use_solver: bool,

    /// Monotonicity constraint for WoE pattern in binning.
    /// Options: "none" (default), "ascending", "descending", "peak", "valley", "auto"
    /// Only applies when --use-solver is enabled.
    #[arg(long, default_value = "none")]
    pub monotonicity: String,

    /// Solver timeout in seconds per feature.
    /// Maximum time allowed for the optimization solver per feature.
    /// Only applies when --use-solver is enabled.
    #[arg(long, default_value = "30")]
    pub solver_timeout: u64,

    /// Solver MIP gap tolerance (0.0 to 1.0).
    /// Solver stops when optimality gap falls below this threshold.
    /// Lower values = more precise but slower. Only applies when --use-solver is enabled.
    #[arg(long, default_value = "0.01", value_parser = validate_solver_gap)]
    pub solver_gap: f64,

    /// Minimum samples per category for categorical features.
    /// Categories with fewer samples are merged into "OTHER".
    #[arg(long, default_value = "5")]
    pub min_category_samples: usize,

    /// Minimum bin size as percentage of total samples for CART binning (0-100).
    /// Only applies to CART binning strategy; ignored for Quantile.
    /// Example: 5.0 means bins must contain at least 5% of total samples.
    /// Default: 5.0 (5% of total samples)
    #[arg(long, default_value = "5.0", value_parser = validate_cart_min_bin_pct)]
    pub cart_min_bin_pct: f64,

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
    /// Convert CSV or SAS7BDAT file to Parquet format
    Convert {
        /// Input file path (CSV or SAS7BDAT)
        input: PathBuf,

        /// Output file path (optional, defaults to input with .parquet extension)
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

#[allow(dead_code)]
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
            let stem = input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            let extension = input
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("parquet");
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

/// Validator for cart_min_bin_pct parameter
fn validate_cart_min_bin_pct(s: &str) -> Result<f64, String> {
    let value: f64 = s
        .parse()
        .map_err(|_| format!("'{}' is not a valid number", s))?;

    if !(0.0..=100.0).contains(&value) {
        Err(format!(
            "cart_min_bin_pct must be between 0.0 and 100.0, got {}",
            value
        ))
    } else {
        Ok(value)
    }
}

/// Validator for solver_gap parameter
fn validate_solver_gap(s: &str) -> Result<f64, String> {
    let value: f64 = s
        .parse()
        .map_err(|_| format!("'{}' is not a valid number", s))?;

    if !(0.0..=1.0).contains(&value) {
        Err(format!(
            "solver_gap must be between 0.0 and 1.0, got {}",
            value
        ))
    } else {
        Ok(value)
    }
}
