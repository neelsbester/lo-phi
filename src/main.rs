//! Feature Reduction CLI Tool
//!
//! A command-line tool for reducing features in datasets using
//! missing value analysis and correlation-based reduction.

mod cli;
mod pipeline;
mod report;
mod utils;

use anyhow::Result;
use clap::Parser;

use cli::Args;

fn main() -> Result<()> {
    let args = Args::parse();
    
    println!("Feature Reduction CLI v{}", env!("CARGO_PKG_VERSION"));
    println!("Input file: {}", args.input.display());
    println!("Target column: {}", args.target);
    println!("Output file: {}", args.output.display());
    println!("Missing threshold: {:.1}%", args.missing_threshold * 100.0);
    println!("Correlation threshold: {:.2}", args.correlation_threshold);
    
    // TODO: Implement pipeline execution
    
    Ok(())
}
