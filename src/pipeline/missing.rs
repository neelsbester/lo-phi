//! Missing value analysis and reduction

use anyhow::Result;
use polars::prelude::*;

/// Analyze missing values in the dataset using Polars expressions
pub fn analyze_missing_values(lf: &LazyFrame) -> Result<Vec<(String, f64)>> {
    let df = lf.clone().collect()?;
    let row_count = df.height() as f64;

    // Use Polars to compute null counts for all columns at once
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

/// Drop features with high missing values from the dataset
pub fn drop_high_missing_features(lf: LazyFrame, features_to_drop: &[String]) -> LazyFrame {
    if features_to_drop.is_empty() {
        return lf;
    }

    let drop_exprs: Vec<Expr> = features_to_drop
        .iter()
        .map(|name| col(name.as_str()))
        .collect();

    lf.drop(drop_exprs)
}
