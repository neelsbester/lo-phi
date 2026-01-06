//! Correlation-based feature reduction

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use polars::prelude::*;
use rayon::prelude::*;
use std::sync::Arc;

/// Represents a correlated pair of features
#[derive(Debug, Clone)]
pub struct CorrelatedPair {
    pub feature1: String,
    pub feature2: String,
    pub correlation: f64,
}

/// Calculate correlations between numeric columns and find highly correlated pairs
/// Uses weighted Pearson correlation with parallel processing via Rayon
///
/// # Arguments
/// * `df` - Reference to the DataFrame (avoids re-collecting from LazyFrame)
/// * `threshold` - Correlation threshold above which pairs are considered highly correlated
/// * `weights` - Sample weights for weighted correlation calculation
/// * `weight_column` - Optional name of the weight column to exclude from analysis
pub fn find_correlated_pairs(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
) -> Result<Vec<CorrelatedPair>> {

    // Get numeric columns only - cast all to Float64 for correlation calculation
    // Exclude the weight column as it's metadata, not a feature
    let numeric_cols: Vec<String> = df
        .get_columns()
        .iter()
        .filter(|col| {
            col.dtype().is_primitive_numeric()
                && Some(col.name().as_str()) != weight_column
        })
        .map(|col| col.name().to_string())
        .collect();

    let num_cols = numeric_cols.len();

    if num_cols < 2 {
        return Ok(Vec::new());
    }

    // Pre-cast all numeric columns to Float64 for efficient correlation calculation
    let float_columns: Vec<(String, Column)> = numeric_cols
        .iter()
        .filter_map(|col_name| {
            df.column(col_name)
                .ok()
                .and_then(|col| col.cast(&DataType::Float64).ok())
                .map(|col| (col_name.clone(), col))
        })
        .collect();

    // Calculate total number of pairs for progress bar (upper triangle)
    let total_pairs = (num_cols * (num_cols - 1)) / 2;

    // Create progress bar with steady tick for smooth rendering
    let pb = ProgressBar::new(total_pairs as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "   Calculating correlations [{bar:40.cyan/blue}] {pos}/{len} pairs ({percent}%) [{eta}]",
            )
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Generate all pairs (indices for upper triangle)
    let pairs: Vec<(usize, usize)> = (0..num_cols)
        .flat_map(|i| ((i + 1)..num_cols).map(move |j| (i, j)))
        .collect();

    // Wrap weights in Arc for efficient sharing across threads
    let weights_arc = Arc::new(weights.to_vec());

    // Process pairs in parallel using Rayon
    let correlated_pairs: Vec<CorrelatedPair> = pairs
        .par_iter()
        .filter_map(|(i, j)| {
            let (col1_name, col1) = &float_columns[*i];
            let (col2_name, col2) = &float_columns[*j];

            let corr = compute_weighted_pearson_correlation(col1, col2, &weights_arc);

            // Update progress - inc() is thread-safe, steady_tick handles smooth rendering
            pb.inc(1);

            corr.and_then(|c| {
                if c.abs() > threshold && !c.is_nan() {
                    Some(CorrelatedPair {
                        feature1: col1_name.clone(),
                        feature2: col2_name.clone(),
                        correlation: c,
                    })
                } else {
                    None
                }
            })
        })
        .collect();

    pb.finish_with_message(format!(
        "   [OK] Analyzed {} column pairs, found {} correlated",
        total_pairs,
        correlated_pairs.len()
    ));

    // Sort by absolute correlation descending
    let mut sorted_pairs = correlated_pairs;
    sorted_pairs.sort_by(|a, b| {
        b.correlation
            .abs()
            .partial_cmp(&a.correlation.abs())
            .unwrap()
    });

    Ok(sorted_pairs)
}

/// Compute weighted Pearson correlation using weighted Welford's algorithm
///
/// Uses a single-pass algorithm for numerical stability with sample weights.
/// When all weights are equal, this produces identical results to unweighted correlation.
fn compute_weighted_pearson_correlation(s1: &Column, s2: &Column, weights: &[f64]) -> Option<f64> {
    let ca1 = s1.f64().ok()?;
    let ca2 = s2.f64().ok()?;

    let n = ca1.len();
    if n == 0 || n != ca2.len() || n != weights.len() {
        return None;
    }

    // Single-pass weighted Welford algorithm for numerical stability
    let mut sum_w = 0.0;
    let mut mean_x = 0.0;
    let mut mean_y = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    let mut cov_xy = 0.0;

    for ((x, y), &w) in ca1.iter().zip(ca2.iter()).zip(weights.iter()) {
        if let (Some(x), Some(y)) = (x, y) {
            if w <= 0.0 {
                continue; // Skip zero or negative weights
            }
            sum_w += w;
            let dx = x - mean_x;
            let dy = y - mean_y;
            mean_x += (w / sum_w) * dx;
            mean_y += (w / sum_w) * dy;
            // Update variances and covariance using weighted Welford's method
            var_x += w * dx * (x - mean_x);
            var_y += w * dy * (y - mean_y);
            cov_xy += w * dx * (y - mean_y);
        }
    }

    // Need at least 2 samples worth of weight
    if sum_w <= 0.0 {
        return None;
    }

    // Use population variance (divided by sum_w, not sum_w - 1) for weighted case
    // This is the standard approach for weighted Pearson correlation
    let std_x = (var_x / sum_w).sqrt();
    let std_y = (var_y / sum_w).sqrt();

    if std_x == 0.0 || std_y == 0.0 {
        return None;
    }

    Some(cov_xy / (sum_w * std_x * std_y))
}

/// Determine which features to drop from correlated pairs
/// Strategy: For each pair, drop the feature that appears more frequently in correlations
pub fn select_features_to_drop(pairs: &[CorrelatedPair], target_column: &str) -> Vec<String> {
    use std::collections::HashMap;

    let mut frequency: HashMap<String, usize> = HashMap::new();

    for pair in pairs {
        *frequency.entry(pair.feature1.clone()).or_insert(0) += 1;
        *frequency.entry(pair.feature2.clone()).or_insert(0) += 1;
    }

    let mut to_drop = Vec::new();
    let mut already_resolved = std::collections::HashSet::new();

    for pair in pairs {
        if already_resolved.contains(&pair.feature1) || already_resolved.contains(&pair.feature2) {
            continue;
        }

        // Never drop the target column
        if pair.feature1 == target_column {
            to_drop.push(pair.feature2.clone());
            already_resolved.insert(pair.feature2.clone());
        } else if pair.feature2 == target_column {
            to_drop.push(pair.feature1.clone());
            already_resolved.insert(pair.feature1.clone());
        } else {
            // Drop the one with higher frequency (more correlations)
            let freq1 = frequency.get(&pair.feature1).unwrap_or(&0);
            let freq2 = frequency.get(&pair.feature2).unwrap_or(&0);

            if freq1 >= freq2 {
                to_drop.push(pair.feature1.clone());
                already_resolved.insert(pair.feature1.clone());
            } else {
                to_drop.push(pair.feature2.clone());
                already_resolved.insert(pair.feature2.clone());
            }
        }
    }

    to_drop
}

