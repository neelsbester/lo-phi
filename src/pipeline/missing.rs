//! Missing value analysis and reduction

use anyhow::Result;
use polars::prelude::*;

/// Analyze missing values in the dataset with optional sample weights.
///
/// When weights are provided, calculates the weighted missing ratio:
/// `weighted_null_count / total_weight` instead of `null_count / row_count`
///
/// # Arguments
/// * `df` - Reference to the DataFrame
/// * `weights` - Sample weights (one per row). Use equal weights for unweighted analysis.
/// * `weight_column` - Optional name of the weight column to exclude from analysis
pub fn analyze_missing_values(
    df: &DataFrame,
    weights: &[f64],
    weight_column: Option<&str>,
) -> Result<Vec<(String, f64)>> {
    // Handle empty DataFrame
    if df.height() == 0 {
        return Ok(Vec::new());
    }

    let total_weight: f64 = weights.iter().sum();

    if total_weight == 0.0 {
        anyhow::bail!("Total weight is zero - cannot compute missing ratios");
    }

    let mut missing_ratios: Vec<(String, f64)> = Vec::new();

    for col_name in df.get_column_names() {
        // Skip the weight column - it's metadata, not a feature
        if Some(col_name.as_str()) == weight_column {
            continue;
        }

        let column = df.column(col_name)?;

        // Calculate weighted null count by iterating through values
        let weighted_null_count: f64 = column
            .as_materialized_series()
            .iter()
            .zip(weights.iter())
            .filter_map(|(val, &w)| if val.is_null() { Some(w) } else { None })
            .sum();

        let missing_ratio = weighted_null_count / total_weight;
        missing_ratios.push((col_name.to_string(), missing_ratio));
    }

    // Sort by missing ratio descending
    missing_ratios.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

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
