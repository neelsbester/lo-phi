//! Command-line argument definitions using clap

use clap::Parser;
use std::path::PathBuf;

/// Feature Reduction CLI - Reduce dataset features using missing value and correlation analysis
#[derive(Parser, Debug)]
#[command(name = "feature-reduce")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Input file path (CSV or Parquet)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Target column name (preserved during reduction)
    #[arg(short, long)]
    pub target: String,

    /// Output file path (CSV or Parquet, determined by extension)
    #[arg(short, long)]
    pub output: PathBuf,

    /// Missing value threshold - drop features with missing values above this ratio
    #[arg(long, default_value = "0.3")]
    pub missing_threshold: f64,

    /// Correlation threshold - drop one feature from pairs with correlation above this value
    #[arg(long, default_value = "0.95")]
    pub correlation_threshold: f64,

    /// Skip interactive confirmation prompts
    #[arg(long, default_value = "false")]
    pub no_confirm: bool,
}

