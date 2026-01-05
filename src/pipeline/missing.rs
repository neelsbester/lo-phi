//! Missing value analysis and reduction

use anyhow::Result;
use polars::prelude::*;

/// Analyze missing values in the dataset using Polars expressions
/// 
/// # Arguments
/// * `df` - Reference to the DataFrame (avoids re-collecting from LazyFrame)
pub fn analyze_missing_values(df: &DataFrame) -> Result<Vec<(String, f64)>> {
    let row_count = df.height() as f64;

    // Use Polars to compute null counts for all columns at once
    // No need to clone - we use a reference and create a new lazy frame
    let null_counts = df
        .clone()
        .lazy()
        .select([all().null_count()])
        .collect()?;

    let mut missing_ratios: Vec<(String, f64)> = Vec::new();

    for col_name in df.get_column_names() {
        let null_count = null_counts
            .column(col_name)?
            .u32()?
            .get(0)
            .unwrap_or(0) as f64;
        let missing_ratio = null_count / row_count;
        missing_ratios.push((col_name.to_string(), missing_ratio));
    }

    // Sort by missing ratio descending
    missing_ratios.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    Ok(missing_ratios)
}

/// Get features to drop based on missing value threshold
pub fn get_features_above_threshold(
    missing_ratios: &[(String, f64)],
    threshold: f64,
    target_column: &str,
) -> Vec<String> {
    missing_ratios
        .iter()
        .filter(|(name, ratio)| *ratio > threshold && name != target_column)
        .map(|(name, _)| name.clone())
        .collect()
}
