//! Correlation-based feature reduction

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use polars::prelude::*;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Represents a correlated pair of features
#[derive(Debug, Clone)]
pub struct CorrelatedPair {
    pub feature1: String,
    pub feature2: String,
    pub correlation: f64,
}

/// Calculate correlations between numeric columns and find highly correlated pairs
/// Uses Polars' optimized pearson correlation with parallel processing via Rayon
pub fn find_correlated_pairs(lf: &LazyFrame, threshold: f64) -> Result<Vec<CorrelatedPair>> {
    let df = lf.clone().collect()?;

    // Get numeric columns only - cast all to Float64 for correlation calculation
    let numeric_cols: Vec<String> = df
        .get_columns()
        .iter()
        .filter(|col| col.dtype().is_primitive_numeric())
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

    // Create progress bar
    let pb = ProgressBar::new(total_pairs as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "   Calculating correlations [{bar:40.cyan/blue}] {pos}/{len} pairs ({percent}%) [{eta}]",
            )
            .unwrap()
            .progress_chars("=>-"),
    );

    // Generate all pairs (indices for upper triangle)
    let pairs: Vec<(usize, usize)> = (0..num_cols)
        .flat_map(|i| ((i + 1)..num_cols).map(move |j| (i, j)))
        .collect();

    // Atomic counter for progress updates
    let progress_counter = Arc::new(AtomicU64::new(0));

    // Process pairs in parallel using Rayon
    let correlated_pairs: Vec<CorrelatedPair> = pairs
        .par_iter()
        .filter_map(|(i, j)| {
            let (col1_name, col1) = &float_columns[*i];
            let (col2_name, col2) = &float_columns[*j];

            let corr = compute_pearson_correlation(col1, col2);

            // Update progress periodically
            let count = progress_counter.fetch_add(1, Ordering::Relaxed);
            if count % 1000 == 0 || count == (total_pairs as u64 - 1) {
                pb.set_position(count + 1);
            }

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

/// Compute Pearson correlation using Polars' optimized single-pass algorithm
fn compute_pearson_correlation(s1: &Column, s2: &Column) -> Option<f64> {
    let ca1 = s1.f64().ok()?;
    let ca2 = s2.f64().ok()?;

    let n = ca1.len();
    if n == 0 || n != ca2.len() {
        return None;
    }

    // Single-pass Welford-style algorithm for numerical stability
    let mut mean_x = 0.0;
    let mut mean_y = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    let mut cov_xy = 0.0;
    let mut count = 0.0;

    for (x, y) in ca1.iter().zip(ca2.iter()) {
        if let (Some(x), Some(y)) = (x, y) {
            count += 1.0;
            let dx = x - mean_x;
            let dy = y - mean_y;
            mean_x += dx / count;
            mean_y += dy / count;
            // Update variances and covariance using Welford's method
            var_x += dx * (x - mean_x);
            var_y += dy * (y - mean_y);
            cov_xy += dx * (y - mean_y);
        }
    }

    if count < 2.0 {
        return None;
    }

    let std_x = (var_x / (count - 1.0)).sqrt();
    let std_y = (var_y / (count - 1.0)).sqrt();

    if std_x == 0.0 || std_y == 0.0 {
        return None;
    }

    Some(cov_xy / ((count - 1.0) * std_x * std_y))
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

/// Drop correlated features from the dataset
pub fn drop_correlated_features(lf: LazyFrame, features_to_drop: &[String]) -> LazyFrame {
    if features_to_drop.is_empty() {
        return lf;
    }

    let drop_exprs: Vec<Expr> = features_to_drop
        .iter()
        .map(|name| col(name.as_str()))
        .collect();

    lf.drop(drop_exprs)
}
