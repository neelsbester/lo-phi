//! Correlation-based feature reduction
//!
//! Supports three association measures:
//! - **Pearson** (|r|) for numeric-numeric pairs
//! - **Bias-corrected Cramér's V** for categorical-categorical pairs
//! - **Eta** (correlation ratio) for categorical-numeric pairs

use anyhow::Result;
use faer::Mat;
use indicatif::{ProgressBar, ProgressStyle};
use polars::prelude::*;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;

use super::iv::FeatureType;
use super::progress::{PipelineStage, ProgressEvent, ProgressSender};

/// Maximum unique categories before a categorical column is excluded from
/// association analysis.  High-cardinality columns (e.g. postal_code) produce
/// unreliable Cramér's V / Eta and are expensive to compute.
const MAX_CATEGORIES: usize = 100;

/// The type of association measure used for a correlated pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AssociationMeasure {
    /// Absolute Pearson correlation coefficient (numeric-numeric)
    Pearson,
    /// Bias-corrected Cramér's V (categorical-categorical)
    CramersV,
    /// Correlation ratio η (categorical-numeric)
    Eta,
}

impl std::fmt::Display for AssociationMeasure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssociationMeasure::Pearson => write!(f, "Pearson"),
            AssociationMeasure::CramersV => write!(f, "CramersV"),
            AssociationMeasure::Eta => write!(f, "Eta"),
        }
    }
}

/// Metadata about a feature used for IV-first drop tie-breaking.
#[derive(Debug, Clone, Default)]
pub struct FeatureMetadata {
    pub iv: Option<f64>,
    pub missing_ratio: Option<f64>,
}

/// A feature selected for dropping with its reason.
#[derive(Debug, Clone)]
pub struct FeatureToDrop {
    pub feature: String,
    pub reason: String,
}

/// Represents a correlated pair of features
#[derive(Debug, Clone)]
pub struct CorrelatedPair {
    pub feature1: String,
    pub feature2: String,
    pub correlation: f64,
    /// The association measure used to compute `correlation`.
    pub measure: AssociationMeasure,
}

/// Calculate correlations between numeric columns and find highly correlated pairs
/// Uses weighted Pearson correlation with parallel processing via Rayon
///
/// # Arguments
/// * `df` - Reference to the DataFrame (avoids re-collecting from LazyFrame)
/// * `threshold` - Correlation threshold above which pairs are considered highly correlated
/// * `weights` - Sample weights for weighted correlation calculation
/// * `weight_column` - Optional name of the weight column to exclude from analysis
#[allow(dead_code)]
pub fn find_correlated_pairs(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
) -> Result<Vec<CorrelatedPair>> {
    find_correlated_pairs_impl(df, threshold, weights, weight_column, false)
}

fn find_correlated_pairs_impl(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
    silent: bool,
) -> Result<Vec<CorrelatedPair>> {
    if df.height() == 0 {
        return Ok(Vec::new());
    }

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
            match df
                .column(col_name)
                .and_then(|col| col.cast(&DataType::Float64))
            {
                Ok(col) => Some((col_name.clone(), col)),
                Err(e) => {
                    eprintln!(
                        "Warning: Excluding column '{}' from correlation analysis: {}",
                        col_name, e
                    );
                    None
                }
            }
        })
        .collect();

    // Calculate total number of pairs for progress bar (upper triangle)
    let total_pairs = (num_cols * (num_cols - 1)) / 2;

    // In TUI mode (silent), use a hidden progress bar so indicatif doesn't
    // write to stdout — ratatui owns the alternate screen.
    let pb = if silent {
        ProgressBar::hidden()
    } else {
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
        pb
    };

    // Generate all pairs (indices for upper triangle)
    let pairs: Vec<(usize, usize)> = (0..num_cols)
        .flat_map(|i| ((i + 1)..num_cols).map(move |j| (i, j)))
        .collect();

    // Process pairs in parallel using Rayon
    // weights is &[f64] which is Send+Sync — safe to share across Rayon threads.
    let correlated_pairs: Vec<CorrelatedPair> = pairs
        .par_iter()
        .filter_map(|(i, j)| {
            let (col1_name, col1) = &float_columns[*i];
            let (col2_name, col2) = &float_columns[*j];

            let corr = compute_weighted_pearson_correlation(col1, col2, weights);

            // Update progress - inc() is thread-safe, steady_tick handles smooth rendering
            pb.inc(1);

            corr.and_then(|c| {
                if c.abs() > threshold && !c.is_nan() {
                    Some(CorrelatedPair {
                        feature1: col1_name.clone(),
                        feature2: col2_name.clone(),
                        correlation: c,
                        measure: AssociationMeasure::Pearson,
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

    if std_x.abs() < f64::EPSILON || std_y.abs() < f64::EPSILON {
        return None;
    }

    Some(cov_xy / (sum_w * std_x * std_y))
}

/// Compute bias-corrected Cramér's V for two categorical columns.
///
/// Uses the Bergsma (2013) bias correction to avoid inflated V for small samples
/// or high cardinality.  Returns a value in [0, 1].
///
/// Returns `None` when the computation is undefined (e.g. single category, no data).
///
/// Accepts pre-cast String columns to avoid redundant casting in hot loops.
pub fn compute_cramers_v(col_a: &Column, col_b: &Column, weights: Option<&[f64]>) -> Option<f64> {
    let n_rows = col_a.len();
    if n_rows == 0 || n_rows != col_b.len() {
        return None;
    }

    let ca = col_a.str().ok()?;
    let cb = col_b.str().ok()?;

    // Single-pass: build category indices, contingency table (flat), and marginals
    // simultaneously.  Uses &str borrows from the chunked arrays (zero heap allocs).
    let mut a_idx: HashMap<&str, usize> = HashMap::new();
    let mut b_idx: HashMap<&str, usize> = HashMap::new();

    // Pre-scan to determine category counts (avoids table resizing)
    for val in ca.iter().flatten() {
        let len = a_idx.len();
        a_idx.entry(val).or_insert(len);
    }
    for val in cb.iter().flatten() {
        let len = b_idx.len();
        b_idx.entry(val).or_insert(len);
    }

    let r = a_idx.len(); // rows
    let c = b_idx.len(); // cols

    if r < 2 || c < 2 {
        return Some(0.0); // No association possible with a single category
    }

    // Flat contingency table (row-major) + marginals computed in one pass
    let mut table = vec![0.0_f64; r * c];
    let mut row_sums = vec![0.0_f64; r];
    let mut col_sums = vec![0.0_f64; c];
    let mut total = 0.0_f64;

    for i in 0..n_rows {
        if let (Some(a), Some(b)) = (ca.get(i), cb.get(i)) {
            let w = weights.map_or(1.0, |ws| ws[i]);
            if w <= 0.0 {
                continue;
            }
            let ri = a_idx[a];
            let ci = b_idx[b];
            table[ri * c + ci] += w;
            row_sums[ri] += w;
            col_sums[ci] += w;
            total += w;
        }
    }

    if total <= 0.0 {
        return None;
    }

    // Chi-squared statistic (cache-friendly row-major traversal)
    let mut chi2 = 0.0_f64;
    for (ri, &rs) in row_sums.iter().enumerate() {
        let row_start = ri * c;
        for (ci, &cs) in col_sums.iter().enumerate() {
            let observed = table[row_start + ci];
            let expected = rs * cs / total;
            if expected > 0.0 {
                let diff = observed - expected;
                chi2 += diff * diff / expected;
            }
        }
    }

    // Bergsma (2013) bias correction
    let n = total;
    let rf = r as f64;
    let cf = c as f64;

    let chi2_corr = (chi2 / n - (rf - 1.0) * (cf - 1.0) / (n - 1.0)).max(0.0);
    let r_corr = rf - (rf - 1.0).powi(2) / (n - 1.0);
    let c_corr = cf - (cf - 1.0).powi(2) / (n - 1.0);

    let denom = (r_corr - 1.0).min(c_corr - 1.0);
    if denom <= 0.0 {
        return Some(0.0);
    }

    Some((chi2_corr / denom).sqrt().clamp(0.0, 1.0))
}

/// Compute the correlation ratio η (eta) for a categorical-numeric pair.
///
/// η measures how much of the numeric variance is explained by the categorical
/// grouping.  Returns a value in [0, 1] (the square root of eta-squared).
///
/// Returns `None` when the computation is undefined (e.g. zero variance, no data).
///
/// Accepts pre-cast columns (String for categorical, Float64 for numeric) to
/// avoid redundant casting in hot loops.
pub fn compute_eta(categorical: &Column, numeric: &Column, weights: Option<&[f64]>) -> Option<f64> {
    let n_rows = categorical.len();
    if n_rows == 0 || n_rows != numeric.len() {
        return None;
    }

    let cat = categorical.str().ok()?;
    let num = numeric.f64().ok()?;

    // Group numeric values by categorical level using &str borrows (zero heap allocs)
    struct GroupStats {
        mean: f64,
        weight: f64,
    }
    let mut groups: HashMap<&str, GroupStats> = HashMap::new();

    // Global weighted mean (Welford)
    let mut global_mean = 0.0_f64;
    let mut global_w = 0.0_f64;
    let mut ss_total = 0.0_f64;

    for i in 0..n_rows {
        if let (Some(c), Some(x)) = (cat.get(i), num.get(i)) {
            let w = weights.map_or(1.0, |ws| ws[i]);
            if w <= 0.0 {
                continue;
            }

            // Update global stats (Welford)
            global_w += w;
            let dx = x - global_mean;
            global_mean += (w / global_w) * dx;
            ss_total += w * dx * (x - global_mean);

            // Update group stats (Welford per group)
            let grp = groups.entry(c).or_insert(GroupStats {
                mean: 0.0,
                weight: 0.0,
            });
            grp.weight += w;
            let gdx = x - grp.mean;
            grp.mean += (w / grp.weight) * gdx;
        }
    }

    if global_w <= 0.0 || ss_total <= 0.0 || groups.len() < 2 {
        return Some(0.0);
    }

    // SS_between = SUM_k(w_k * (mean_k - global_mean)^2)
    let ss_between: f64 = groups
        .values()
        .map(|g| g.weight * (g.mean - global_mean).powi(2))
        .sum();

    let eta_sq = ss_between / ss_total;
    Some(eta_sq.sqrt().clamp(0.0, 1.0))
}

/// Classify DataFrame columns into numeric and categorical.
///
/// Uses the same heuristic as IV analysis: `is_primitive_numeric()` for numeric,
/// `String | Categorical` for categorical.
fn classify_columns(
    df: &DataFrame,
    weight_column: Option<&str>,
    feature_types: Option<&HashMap<String, FeatureType>>,
) -> (Vec<String>, Vec<String>) {
    let mut numeric = Vec::new();
    let mut categorical = Vec::new();

    for col in df.get_columns() {
        let name = col.name().as_str();
        if Some(name) == weight_column {
            continue;
        }

        // If the caller provided a feature_types map (from IV stage), use it;
        // otherwise fall back to dtype inspection.
        let is_cat = if let Some(ft_map) = feature_types {
            ft_map.get(name) == Some(&FeatureType::Categorical)
        } else {
            matches!(col.dtype(), DataType::String | DataType::Categorical(_, _))
        };

        if is_cat {
            categorical.push(name.to_string());
        } else if col.dtype().is_primitive_numeric() {
            numeric.push(name.to_string());
        }
        // Other types (dates, booleans, etc.) are silently skipped.
    }

    (numeric, categorical)
}

/// Compute correlation matrix using matrix operations (much faster for many columns).
///
/// Algorithm:
/// 1. Build data matrix X (n_rows x n_cols) from numeric columns
/// 2. Compute weighted means and standardize: Z = (X - mean) / std
/// 3. Compute correlation matrix: R = Z^T * diag(W) * Z / sum(W)
///
/// Returns the correlation matrix and column names.
fn compute_correlation_matrix_fast(
    float_columns: &[(String, Column)],
    weights: &[f64],
) -> Result<(Mat<f64>, Vec<String>)> {
    let n_cols = float_columns.len();
    if n_cols < 2 {
        anyhow::bail!(
            "Need at least 2 columns to compute a correlation matrix, got {}",
            n_cols
        );
    }

    // Extract column names
    let col_names: Vec<String> = float_columns.iter().map(|(name, _)| name.clone()).collect();

    // Get row count from first column
    let n_rows = float_columns[0].1.len();
    if n_rows == 0 {
        anyhow::bail!("Cannot compute correlation matrix: dataset has no rows");
    }
    if weights.len() != n_rows {
        anyhow::bail!(
            "Weight vector length ({}) does not match number of rows ({})",
            weights.len(),
            n_rows
        );
    }

    // Compute total weight
    let sum_w: f64 = weights.iter().sum();
    if sum_w <= 0.0 {
        anyhow::bail!("Cannot compute correlation matrix: total weight is zero or negative");
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
        anyhow::bail!(
            "Need at least 2 non-constant columns for correlation matrix, but only {} valid columns remain after excluding constant/all-null columns",
            n_valid_cols
        );
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

    Ok((corr_matrix, valid_col_names))
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
                    measure: AssociationMeasure::Pearson,
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
#[allow(dead_code)]
pub fn find_correlated_pairs_matrix(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
) -> Result<Vec<CorrelatedPair>> {
    find_correlated_pairs_matrix_impl(df, threshold, weights, weight_column, false)
}

fn find_correlated_pairs_matrix_impl(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
    silent: bool,
) -> Result<Vec<CorrelatedPair>> {
    if df.height() == 0 {
        return Ok(Vec::new());
    }

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
            match df
                .column(col_name)
                .and_then(|col| col.cast(&DataType::Float64))
            {
                Ok(col) => Some((col_name.clone(), col)),
                Err(e) => {
                    eprintln!(
                        "Warning: Excluding column '{}' from correlation analysis: {}",
                        col_name, e
                    );
                    None
                }
            }
        })
        .collect();

    // In TUI mode (silent), use a hidden progress bar so indicatif doesn't
    // write to stdout — ratatui owns the alternate screen.
    let pb = if silent {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("   {spinner:.cyan} Computing correlation matrix ({msg})")
                .unwrap(),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb
    };
    pb.set_message(format!("{} columns", float_columns.len()));

    // Compute correlation matrix
    let (corr_matrix, col_names) = compute_correlation_matrix_fast(&float_columns, weights)?;

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
/// - Matrix method: Used when numeric columns >= 15 (better for many columns)
/// - Pairwise method: Used when numeric columns < 15 (lower overhead for few columns)
///
/// Also computes cat-cat (Cramér's V) and cat-num (Eta) pairs when categorical
/// columns are present.
pub fn find_correlated_pairs_auto(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
    feature_types: Option<&HashMap<String, FeatureType>>,
) -> Result<Vec<CorrelatedPair>> {
    find_correlated_pairs_auto_impl(df, threshold, weights, weight_column, feature_types, None)
}

/// Same as `find_correlated_pairs_auto` but sends progress events to the TUI overlay.
pub fn find_correlated_pairs_auto_with_progress(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
    feature_types: Option<&HashMap<String, FeatureType>>,
    progress_tx: &ProgressSender,
) -> Result<Vec<CorrelatedPair>> {
    find_correlated_pairs_auto_impl(
        df,
        threshold,
        weights,
        weight_column,
        feature_types,
        Some(progress_tx),
    )
}

fn find_correlated_pairs_auto_impl(
    df: &DataFrame,
    threshold: f64,
    weights: &[f64],
    weight_column: Option<&str>,
    feature_types: Option<&HashMap<String, FeatureType>>,
    progress_tx: Option<&ProgressSender>,
) -> Result<Vec<CorrelatedPair>> {
    let (numeric_cols, all_cat_cols) = classify_columns(df, weight_column, feature_types);
    let num_count = numeric_cols.len();

    // When a progress channel is provided we're in TUI mode — indicatif
    // progress bars must not write to stdout because ratatui owns the
    // alternate screen.  Pass `silent = true` so the inner functions use
    // ProgressBar::hidden() instead.
    let silent = progress_tx.is_some();

    // Pre-cast categorical columns to String once (not per-pair).
    // Also applies early-exit cardinality check: stop counting after
    // MAX_CATEGORIES+1 unique values instead of scanning the entire column.
    let cat_str_columns: Vec<(String, Column)> = all_cat_cols
        .into_iter()
        .filter_map(|name| {
            let col = df.column(&name).ok()?;
            let cast = col.cast(&DataType::String).ok()?;
            let ca = cast.str().ok()?;
            // Early-exit cardinality check — much faster than n_unique() for
            // high-cardinality columns (exits after ~100 rows, not N rows).
            let mut seen = std::collections::HashSet::new();
            for val in ca.iter().flatten() {
                seen.insert(val);
                if seen.len() > MAX_CATEGORIES {
                    if !silent {
                        eprintln!(
                            "Warning: Skipping categorical column '{}' from association analysis (>{} unique categories)",
                            name, MAX_CATEGORIES
                        );
                    }
                    return None;
                }
            }
            Some((name, cast))
        })
        .collect();

    let cat_count = cat_str_columns.len();

    // Pre-cast numeric columns to Float64 once for Eta computations
    let num_f64_columns: Vec<(String, Column)> = numeric_cols
        .iter()
        .filter_map(|name| {
            let col = df.column(name).ok()?;
            let cast = col.cast(&DataType::Float64).ok()?;
            Some((name.clone(), cast))
        })
        .collect();

    let num_num_pairs = (num_count * num_count.saturating_sub(1)) / 2;
    let cat_cat_pairs = (cat_count * cat_count.saturating_sub(1)) / 2;
    let cat_num_pairs = cat_count * num_count;
    let total_pairs = num_num_pairs + cat_cat_pairs + cat_num_pairs;

    if let Some(tx) = progress_tx {
        tx.send(ProgressEvent::update(
            PipelineStage::CorrelationAnalysis,
            "Correlation analysis",
            format!(
                "{} numeric + {} categorical features, {} pairs",
                num_count, cat_count, total_pairs
            ),
        ))
        .ok();
    }

    // ── Num-Num block (existing Pearson logic) ───────────────────────────
    let mut all_pairs = if num_count >= 2 {
        if num_count >= MATRIX_METHOD_COLUMN_THRESHOLD {
            find_correlated_pairs_matrix_impl(df, threshold, weights, weight_column, silent)?
        } else {
            find_correlated_pairs_impl(df, threshold, weights, weight_column, silent)?
        }
    } else {
        Vec::new()
    };

    // ── Cat-Cat and Cat-Num blocks (run in parallel via rayon::join) ─────
    let weights_opt: Option<&[f64]> = Some(weights);

    let (cat_results, cn_results) = rayon::join(
        || {
            // Cat-Cat block (Cramér's V) — uses pre-cast String columns
            if cat_count < 2 {
                return Vec::new();
            }
            let cat_pairs_idx: Vec<(usize, usize)> = (0..cat_count)
                .flat_map(|i| ((i + 1)..cat_count).map(move |j| (i, j)))
                .collect();

            cat_pairs_idx
                .par_iter()
                .filter_map(|(i, j)| {
                    let (_, col_a) = &cat_str_columns[*i];
                    let (_, col_b) = &cat_str_columns[*j];
                    let v = compute_cramers_v(col_a, col_b, weights_opt)?;
                    if v > threshold && !v.is_nan() {
                        Some(CorrelatedPair {
                            feature1: cat_str_columns[*i].0.clone(),
                            feature2: cat_str_columns[*j].0.clone(),
                            correlation: v,
                            measure: AssociationMeasure::CramersV,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        },
        || {
            // Cat-Num block (Eta) — uses pre-cast String + Float64 columns
            if cat_str_columns.is_empty() || num_f64_columns.is_empty() {
                return Vec::new();
            }
            let cn_pairs_idx: Vec<(usize, usize)> = (0..cat_count)
                .flat_map(|i| (0..num_f64_columns.len()).map(move |j| (i, j)))
                .collect();

            cn_pairs_idx
                .par_iter()
                .filter_map(|(ci, ni)| {
                    let (_, cat_col) = &cat_str_columns[*ci];
                    let (_, num_col) = &num_f64_columns[*ni];
                    let eta = compute_eta(cat_col, num_col, weights_opt)?;
                    if eta > threshold && !eta.is_nan() {
                        Some(CorrelatedPair {
                            feature1: cat_str_columns[*ci].0.clone(),
                            feature2: num_f64_columns[*ni].0.clone(),
                            correlation: eta,
                            measure: AssociationMeasure::Eta,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        },
    );

    all_pairs.extend(cat_results);
    all_pairs.extend(cn_results);

    // Sort all pairs by absolute correlation descending
    all_pairs.sort_by(|a, b| {
        b.correlation
            .abs()
            .partial_cmp(&a.correlation.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(all_pairs)
}

/// Determine which features to drop from correlated pairs.
///
/// Decision priority (IV-first / modeler_challenger pattern):
/// 1. **Target protection** — never drop the target column
/// 2. **Lower IV** — when metadata available and both features have IV, drop lower IV
/// 3. **Higher frequency** — drop feature appearing in more correlated pairs
/// 4. **Higher missing ratio** — when both have missing ratio, drop higher missingness
/// 5. **Alphabetical** — deterministic fallback
pub fn select_features_to_drop(
    pairs: &[CorrelatedPair],
    target_column: &str,
    metadata: Option<&HashMap<String, FeatureMetadata>>,
) -> Vec<FeatureToDrop> {
    // Use &str borrows from CorrelatedPair fields to avoid per-pair String clones
    let mut frequency: HashMap<&str, usize> = HashMap::new();

    for pair in pairs {
        *frequency.entry(&pair.feature1).or_insert(0) += 1;
        *frequency.entry(&pair.feature2).or_insert(0) += 1;
    }

    let mut to_drop = Vec::new();
    let mut already_resolved: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for pair in pairs {
        if already_resolved.contains(pair.feature1.as_str())
            || already_resolved.contains(pair.feature2.as_str())
        {
            continue;
        }

        let (dropped, reason) = decide_feature_to_drop(
            &pair.feature1,
            &pair.feature2,
            target_column,
            &frequency,
            metadata,
            pair,
        );

        already_resolved.insert(if dropped == pair.feature1 {
            &pair.feature1
        } else {
            &pair.feature2
        });
        to_drop.push(FeatureToDrop {
            feature: dropped,
            reason,
        });
    }

    to_drop
}

/// Waterfall logic for deciding which feature in a pair to drop.
///
/// Returns (feature_to_drop, human-readable reason).
fn decide_feature_to_drop(
    f1: &str,
    f2: &str,
    target_column: &str,
    frequency: &HashMap<&str, usize>,
    metadata: Option<&HashMap<String, FeatureMetadata>>,
    pair: &CorrelatedPair,
) -> (String, String) {
    let measure_label = pair.measure.to_string();
    let coeff = pair.correlation;

    // 1. Target protection
    if f1 == target_column {
        return (
            f2.to_string(),
            format!(
                "Correlated with target ({} {:.4}); dropped to protect target",
                measure_label, coeff
            ),
        );
    }
    if f2 == target_column {
        return (
            f1.to_string(),
            format!(
                "Correlated with target ({} {:.4}); dropped to protect target",
                measure_label, coeff
            ),
        );
    }

    // 2. Lower IV (primary)
    if let Some(meta) = metadata {
        let iv1 = meta.get(f1).and_then(|m| m.iv);
        let iv2 = meta.get(f2).and_then(|m| m.iv);
        if let (Some(iv1_val), Some(iv2_val)) = (iv1, iv2) {
            // Keep the higher-IV feature; drop the lower-IV one
            if (iv1_val - iv2_val).abs() > f64::EPSILON {
                return if iv1_val < iv2_val {
                    (
                        f1.to_string(),
                        format!(
                            "Correlated with {} ({} {:.4}); lower IV ({:.4} vs {:.4})",
                            f2, measure_label, coeff, iv1_val, iv2_val
                        ),
                    )
                } else {
                    (
                        f2.to_string(),
                        format!(
                            "Correlated with {} ({} {:.4}); lower IV ({:.4} vs {:.4})",
                            f1, measure_label, coeff, iv2_val, iv1_val
                        ),
                    )
                };
            }
        }
    }

    // 3. Higher frequency (secondary)
    let freq1 = *frequency.get(f1).unwrap_or(&0);
    let freq2 = *frequency.get(f2).unwrap_or(&0);
    if freq1 != freq2 {
        return if freq1 > freq2 {
            (
                f1.to_string(),
                format!(
                    "Correlated with {} ({} {:.4}); higher frequency ({} vs {})",
                    f2, measure_label, coeff, freq1, freq2
                ),
            )
        } else {
            (
                f2.to_string(),
                format!(
                    "Correlated with {} ({} {:.4}); higher frequency ({} vs {})",
                    f1, measure_label, coeff, freq2, freq1
                ),
            )
        };
    }

    // 4. Higher missing ratio (tertiary)
    if let Some(meta) = metadata {
        let mr1 = meta.get(f1).and_then(|m| m.missing_ratio);
        let mr2 = meta.get(f2).and_then(|m| m.missing_ratio);
        if let (Some(mr1_val), Some(mr2_val)) = (mr1, mr2) {
            if (mr1_val - mr2_val).abs() > f64::EPSILON {
                return if mr1_val > mr2_val {
                    (
                        f1.to_string(),
                        format!(
                            "Correlated with {} ({} {:.4}); higher missing ratio ({:.4} vs {:.4})",
                            f2, measure_label, coeff, mr1_val, mr2_val
                        ),
                    )
                } else {
                    (
                        f2.to_string(),
                        format!(
                            "Correlated with {} ({} {:.4}); higher missing ratio ({:.4} vs {:.4})",
                            f1, measure_label, coeff, mr2_val, mr1_val
                        ),
                    )
                };
            }
        }
    }

    // 5. Alphabetical fallback
    if f1 < f2 {
        (
            f2.to_string(),
            format!(
                "Correlated with {} ({} {:.4}); alphabetical tie-break",
                f1, measure_label, coeff
            ),
        )
    } else {
        (
            f1.to_string(),
            format!(
                "Correlated with {} ({} {:.4}); alphabetical tie-break",
                f2, measure_label, coeff
            ),
        )
    }
}
