//! Correlation-based feature reduction

use anyhow::Result;
use faer::Mat;
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
            col.dtype().is_primitive_numeric() && Some(col.name().as_str()) != weight_column
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
            .unwrap_or(std::cmp::Ordering::Equal)
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

/// Compute correlation matrix using matrix operations (much faster for many columns).
///
/// Algorithm:
/// 1. Build data matrix X (n_rows x n_cols) from numeric columns
/// 2. Compute weighted means and standardize: Z = (X - mean) / std
/// 3. Compute correlation matrix: R = Z^T * diag(W) * Z / sum(W)
///
/// Returns the correlation matrix and column names, or None if computation fails.
fn compute_correlation_matrix_fast(
    float_columns: &[(String, Column)],
    weights: &[f64],
) -> Option<(Mat<f64>, Vec<String>)> {
    let n_cols = float_columns.len();
    if n_cols < 2 {
        return None;
    }

    // Extract column names
    let col_names: Vec<String> = float_columns.iter().map(|(name, _)| name.clone()).collect();

    // Get row count from first column
    let n_rows = float_columns[0].1.len();
    if n_rows == 0 || weights.len() != n_rows {
        return None;
    }

    // Compute total weight
    let sum_w: f64 = weights.iter().sum();
    if sum_w <= 0.0 {
        return None;
    }

    // Build data matrix and compute weighted statistics in parallel
    // For each column: extract values, compute weighted mean/std, standardize
    let standardized_cols: Vec<Option<Vec<f64>>> = float_columns
        .par_iter()
        .map(|(_, col)| {
            let ca = col.f64().ok()?;

            // Extract values, replacing nulls with 0 (will be handled by weight)
            // Compute weighted mean first
            let mut sum_wx = 0.0;
            let mut sum_w_valid = 0.0;

            for (val, &w) in ca.iter().zip(weights.iter()) {
                if let Some(x) = val {
                    if w > 0.0 {
                        sum_wx += w * x;
                        sum_w_valid += w;
                    }
                }
            }

            if sum_w_valid <= 0.0 {
                return None;
            }

            let mean = sum_wx / sum_w_valid;

            // Compute weighted variance
            let mut sum_w_sq_dev = 0.0;
            for (val, &w) in ca.iter().zip(weights.iter()) {
                if let Some(x) = val {
                    if w > 0.0 {
                        let dev = x - mean;
                        sum_w_sq_dev += w * dev * dev;
                    }
                }
            }

            let std = (sum_w_sq_dev / sum_w_valid).sqrt();
            if std == 0.0 {
                return None; // Constant column - skip
            }

            // Standardize: (x - mean) / std, with sqrt(w) applied
            // For weighted correlation: Z_weighted = sqrt(W) * Z
            let standardized: Vec<f64> = ca
                .iter()
                .zip(weights.iter())
                .map(|(val, &w)| {
                    if let Some(x) = val {
                        if w > 0.0 {
                            (w.sqrt() / sum_w_valid.sqrt()) * (x - mean) / std
                        } else {
                            0.0
                        }
                    } else {
                        0.0 // Null values contribute 0 after weighting
                    }
                })
                .collect();

            Some(standardized)
        })
        .collect();

    // Filter out columns that failed (constant or all null)
    let valid_cols: Vec<(usize, Vec<f64>)> = standardized_cols
        .into_iter()
        .enumerate()
        .filter_map(|(i, opt)| opt.map(|v| (i, v)))
        .collect();

    let valid_col_names: Vec<String> = valid_cols
        .iter()
        .map(|(i, _)| col_names[*i].clone())
        .collect();
    let n_valid_cols = valid_cols.len();

    if n_valid_cols < 2 {
        return None;
    }

    // Build the standardized data matrix Z (n_rows x n_valid_cols)
    let mut z = Mat::<f64>::zeros(n_rows, n_valid_cols);
    for (col_idx, (_, col_data)) in valid_cols.iter().enumerate() {
        for (row_idx, &val) in col_data.iter().enumerate() {
            z[(row_idx, col_idx)] = val;
        }
    }

    // Compute correlation matrix: R = Z^T * Z
    // Since we pre-applied sqrt(w)/sqrt(sum_w) to Z, this gives us weighted correlation
    let corr_matrix = z.transpose() * &z;

    Some((corr_matrix, valid_col_names))
}

/// Extract correlated pairs from correlation matrix
fn extract_correlated_pairs_from_matrix(
    corr_matrix: &Mat<f64>,
    col_names: &[String],
    threshold: f64,
) -> Vec<CorrelatedPair> {
    let n = corr_matrix.nrows();
    let mut pairs = Vec::new();

    // Extract upper triangle
    for i in 0..n {
        for j in (i + 1)..n {
            let corr = corr_matrix[(i, j)];
            if corr.abs() > threshold && !corr.is_nan() {
                pairs.push(CorrelatedPair {
                    feature1: col_names[i].clone(),
                    feature2: col_names[j].clone(),
                    correlation: corr,
                });
            }
        }
    }

    // Sort by absolute correlation descending
    pairs.sort_by(|a, b| {
        b.correlation
            .abs()
            .partial_cmp(&a.correlation.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    pairs
}

/// Find correlated pairs using matrix-based computation (optimized for many columns).
///
/// This is significantly faster than pairwise computation when there are many columns
/// because matrix multiplication is highly optimized (O(n²) operations in optimized BLAS
/// vs O(n² * m) for pairwise where m is row count).
pub fn find_correlated_pairs_matrix(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
) -> Result<Vec<CorrelatedPair>> {
    // Get numeric columns only - cast all to Float64 for correlation calculation
    let numeric_cols: Vec<String> = df
        .get_columns()
        .iter()
        .filter(|col| {
            col.dtype().is_primitive_numeric() && Some(col.name().as_str()) != weight_column
        })
        .map(|col| col.name().to_string())
        .collect();

    let num_cols = numeric_cols.len();

    if num_cols < 2 {
        return Ok(Vec::new());
    }

    // Pre-cast all numeric columns to Float64
    let float_columns: Vec<(String, Column)> = numeric_cols
        .iter()
        .filter_map(|col_name| {
            df.column(col_name)
                .ok()
                .and_then(|col| col.cast(&DataType::Float64).ok())
                .map(|col| (col_name.clone(), col))
        })
        .collect();

    // Show progress for matrix computation
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("   {spinner:.cyan} Computing correlation matrix ({msg})")
            .unwrap(),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb.set_message(format!("{} columns", float_columns.len()));

    // Compute correlation matrix
    let (corr_matrix, col_names) = compute_correlation_matrix_fast(&float_columns, weights)
        .ok_or_else(|| anyhow::anyhow!("Failed to compute correlation matrix"))?;

    pb.set_message("extracting pairs");

    // Extract pairs above threshold
    let pairs = extract_correlated_pairs_from_matrix(&corr_matrix, &col_names, threshold);

    pb.finish_with_message(format!(
        "analyzed {} columns, found {} correlated pairs",
        col_names.len(),
        pairs.len()
    ));

    Ok(pairs)
}

/// Threshold for auto-selecting matrix vs pairwise correlation computation.
/// Matrix multiplication is more efficient when there are many columns.
const MATRIX_METHOD_COLUMN_THRESHOLD: usize = 15;

/// Find correlated pairs using auto-selected method (matrix or pairwise).
///
/// Automatically chooses the most efficient method based on dataset characteristics:
/// - Matrix method: Used when columns >= 15 (better for many columns)
/// - Pairwise method: Used when columns < 15 (lower overhead for few columns)
pub fn find_correlated_pairs_auto(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
) -> Result<Vec<CorrelatedPair>> {
    // Count numeric columns
    let num_cols = df
        .get_columns()
        .iter()
        .filter(|col| {
            col.dtype().is_primitive_numeric() && Some(col.name().as_str()) != weight_column
        })
        .count();

    if num_cols >= MATRIX_METHOD_COLUMN_THRESHOLD {
        find_correlated_pairs_matrix(df, threshold, weights, weight_column)
    } else {
        find_correlated_pairs(df, threshold, weights, weight_column)
    }
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
