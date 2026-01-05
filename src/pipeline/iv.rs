//! Information Value (IV) and Weight of Evidence (WoE) based feature selection
//!
//! This module implements IV-optimal binning with greedy merging to calculate
//! the predictive power of features against a binary target.

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use polars::prelude::*;
use rayon::prelude::*;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Number of initial quantile pre-bins before merging
const PRE_BIN_COUNT: usize = 50;

/// Minimum samples per bin to avoid unstable WoE estimates
const MIN_BIN_SAMPLES: usize = 5;

/// Smoothing constant to avoid log(0) in WoE calculation (Laplace smoothing)
const SMOOTHING: f64 = 0.5;

/// A single bin with WoE statistics
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]  // Fields may be used for reporting/debugging
pub struct WoeBin {
    /// Lower bound (inclusive)
    pub lower_bound: f64,
    /// Upper bound (exclusive, except for last bin)
    pub upper_bound: f64,
    /// Count of events (target = 1) in this bin
    pub events: usize,
    /// Count of non-events (target = 0) in this bin
    pub non_events: usize,
    /// Weight of Evidence for this bin
    pub woe: f64,
    /// Contribution to total IV from this bin
    pub iv_contribution: f64,
}

/// Complete IV analysis results for a single feature
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]  // Fields may be used for reporting/debugging
pub struct IvAnalysis {
    /// Name of the analyzed feature
    pub feature_name: String,
    /// Bins with WoE statistics
    pub bins: Vec<WoeBin>,
    /// Total Information Value
    pub iv: f64,
    /// Gini coefficient calculated on WoE-encoded values
    pub gini: f64,
}

/// Analyze all numeric features and calculate their IV
///
/// # Arguments
/// * `df` - Reference to the DataFrame (avoids re-collecting from LazyFrame)
/// * `target` - Name of the binary target column (must contain 0 and 1)
/// * `num_bins` - Target number of bins after merging
///
/// # Returns
/// Vector of IvAnalysis for each numeric feature, sorted by IV descending
pub fn analyze_features_iv(
    df: &DataFrame,
    target: &str,
    num_bins: usize,
) -> Result<Vec<IvAnalysis>> {

    // Validate target column
    validate_binary_target(&df, target)?;

    // Get target values as i32
    let target_col = df.column(target)?;
    let target_values: Vec<i32> = target_col
        .cast(&DataType::Int32)?
        .i32()?
        .into_no_null_iter()
        .collect();

    // Get numeric columns (excluding target)
    let numeric_cols: Vec<String> = df
        .get_columns()
        .iter()
        .filter(|col| col.dtype().is_primitive_numeric() && col.name() != target)
        .map(|col| col.name().to_string())
        .collect();

    let num_features = numeric_cols.len();

    if num_features == 0 {
        return Ok(Vec::new());
    }

    // Create progress bar
    let pb = ProgressBar::new(num_features as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "   Calculating IV [{bar:40.cyan/blue}] {pos}/{len} features ({percent}%) [{eta}]",
            )
            .unwrap()
            .progress_chars("=>-"),
    );

    // Atomic counter for progress
    let progress_counter = Arc::new(AtomicU64::new(0));

    // Process features in parallel
    let analyses: Vec<IvAnalysis> = numeric_cols
        .par_iter()
        .filter_map(|col_name| {
            let result = analyze_single_feature(&df, col_name, &target_values, num_bins);

            // Update progress
            let count = progress_counter.fetch_add(1, Ordering::Relaxed);
            if count % 10 == 0 || count == (num_features as u64 - 1) {
                pb.set_position(count + 1);
            }

            match result {
                Ok(analysis) => Some(analysis),
                Err(_) => None, // Skip features that fail (e.g., all null)
            }
        })
        .collect();

    pb.finish_with_message(format!(
        "   [OK] Analyzed {} features",
        analyses.len()
    ));

    // Sort by IV descending
    let mut sorted = analyses;
    sorted.sort_by(|a, b| b.iv.partial_cmp(&a.iv).unwrap_or(std::cmp::Ordering::Equal));

    Ok(sorted)
}

/// Validate that the target column is binary (contains only 0 and 1)
fn validate_binary_target(df: &DataFrame, target: &str) -> Result<()> {
    // NOTE: df is already borrowed, no collection needed
    let target_col = df
        .column(target)
        .with_context(|| format!("Target column '{}' not found", target))?;

    let unique = target_col.unique()?.cast(&DataType::Int32)?;
    let unique_values: Vec<i32> = unique.i32()?.into_no_null_iter().collect();

    let valid = unique_values.len() <= 2
        && unique_values.iter().all(|&v| v == 0 || v == 1);

    if !valid {
        anyhow::bail!(
            "Target column '{}' must be binary (0/1). Found values: {:?}",
            target,
            unique_values
        );
    }

    Ok(())
}

/// Analyze a single feature and calculate its IV
fn analyze_single_feature(
    df: &DataFrame,
    col_name: &str,
    target_values: &[i32],
    num_bins: usize,
) -> Result<IvAnalysis> {
    let col = df.column(col_name)?;
    let float_col = col.cast(&DataType::Float64)?;
    let values = float_col.f64()?;

    // Collect non-null value/target pairs
    let mut pairs: Vec<(f64, i32)> = values
        .iter()
        .zip(target_values.iter())
        .filter_map(|(v, &t)| v.map(|val| (val, t)))
        .collect();

    if pairs.len() < MIN_BIN_SAMPLES * 2 {
        anyhow::bail!("Insufficient non-null values for feature '{}'", col_name);
    }

    // Sort by value for binning
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Count total events and non-events
    let total_events: usize = pairs.iter().filter(|(_, t)| *t == 1).count();
    let total_non_events: usize = pairs.len() - total_events;

    if total_events == 0 || total_non_events == 0 {
        anyhow::bail!(
            "Feature '{}' has no variation in target (all 0s or all 1s)",
            col_name
        );
    }

    // Phase 1: Create initial pre-bins using quantiles
    let pre_bins = create_quantile_prebins(&pairs, PRE_BIN_COUNT, total_events, total_non_events);

    // Phase 2: Greedy merge until target bin count
    let final_bins = greedy_merge_bins(pre_bins, num_bins, total_events, total_non_events);

    // Calculate total IV
    let iv: f64 = final_bins.iter().map(|b| b.iv_contribution).sum();

    // Calculate Gini on WoE-encoded values
    let gini = calculate_gini_on_woe(&pairs, &final_bins, target_values.len());

    Ok(IvAnalysis {
        feature_name: col_name.to_string(),
        bins: final_bins,
        iv,
        gini,
    })
}

/// Create initial quantile-based pre-bins
fn create_quantile_prebins(
    sorted_pairs: &[(f64, i32)],
    num_prebins: usize,
    total_events: usize,
    total_non_events: usize,
) -> Vec<WoeBin> {
    let n = sorted_pairs.len();
    let bin_size = (n + num_prebins - 1) / num_prebins; // Ceiling division

    let mut bins = Vec::new();
    let mut start_idx = 0;

    while start_idx < n {
        let end_idx = (start_idx + bin_size).min(n);
        let bin_pairs = &sorted_pairs[start_idx..end_idx];

        let lower = bin_pairs.first().map(|(v, _)| *v).unwrap_or(f64::NEG_INFINITY);
        let upper = if end_idx < n {
            sorted_pairs[end_idx].0
        } else {
            f64::INFINITY
        };

        let events: usize = bin_pairs.iter().filter(|(_, t)| *t == 1).count();
        let non_events = bin_pairs.len() - events;

        let (woe, iv_contrib) =
            calculate_woe_iv(events, non_events, total_events, total_non_events);

        bins.push(WoeBin {
            lower_bound: lower,
            upper_bound: upper,
            events,
            non_events,
            woe,
            iv_contribution: iv_contrib,
        });

        start_idx = end_idx;
    }

    bins
}

/// Calculate WoE and IV contribution for a bin
fn calculate_woe_iv(
    events: usize,
    non_events: usize,
    total_events: usize,
    total_non_events: usize,
) -> (f64, f64) {
    // Apply Laplace smoothing to avoid log(0)
    let dist_events = (events as f64 + SMOOTHING) / (total_events as f64 + SMOOTHING);
    let dist_non_events = (non_events as f64 + SMOOTHING) / (total_non_events as f64 + SMOOTHING);

    let woe = (dist_non_events / dist_events).ln();
    let iv_contrib = (dist_non_events - dist_events) * woe;

    (woe, iv_contrib)
}

/// Greedy merge bins to minimize IV loss until target bin count is reached
fn greedy_merge_bins(
    mut bins: Vec<WoeBin>,
    target_bins: usize,
    total_events: usize,
    total_non_events: usize,
) -> Vec<WoeBin> {
    while bins.len() > target_bins && bins.len() > 1 {
        // Find the adjacent pair whose merge results in minimum IV loss
        let mut min_loss = f64::MAX;
        let mut merge_idx = 0;

        for i in 0..bins.len() - 1 {
            let merged = merge_two_bins(&bins[i], &bins[i + 1], total_events, total_non_events);
            let current_iv = bins[i].iv_contribution + bins[i + 1].iv_contribution;
            let new_iv = merged.iv_contribution;
            let loss = current_iv - new_iv;

            if loss < min_loss {
                min_loss = loss;
                merge_idx = i;
            }
        }

        // Perform the merge
        let merged = merge_two_bins(
            &bins[merge_idx],
            &bins[merge_idx + 1],
            total_events,
            total_non_events,
        );
        bins.remove(merge_idx + 1);
        bins[merge_idx] = merged;
    }

    bins
}

/// Merge two adjacent bins into one
fn merge_two_bins(
    bin1: &WoeBin,
    bin2: &WoeBin,
    total_events: usize,
    total_non_events: usize,
) -> WoeBin {
    let events = bin1.events + bin2.events;
    let non_events = bin1.non_events + bin2.non_events;
    let (woe, iv_contrib) = calculate_woe_iv(events, non_events, total_events, total_non_events);

    WoeBin {
        lower_bound: bin1.lower_bound,
        upper_bound: bin2.upper_bound,
        events,
        non_events,
        woe,
        iv_contribution: iv_contrib,
    }
}

/// Calculate Gini coefficient on WoE-encoded values using AUC
fn calculate_gini_on_woe(
    sorted_pairs: &[(f64, i32)],
    bins: &[WoeBin],
    _total_samples: usize,
) -> f64 {
    // Encode each value with its bin's WoE
    let mut woe_target_pairs: Vec<(f64, i32)> = sorted_pairs
        .iter()
        .map(|(val, target)| {
            let woe = find_woe_for_value(*val, bins);
            (woe, *target)
        })
        .collect();

    // Sort by WoE for AUC calculation
    woe_target_pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Calculate AUC using Mann-Whitney U statistic
    let auc = calculate_auc(&woe_target_pairs);

    // Gini = 2 * AUC - 1
    2.0 * auc - 1.0
}

/// Find the WoE value for a given feature value
fn find_woe_for_value(value: f64, bins: &[WoeBin]) -> f64 {
    for bin in bins {
        if value >= bin.lower_bound && value < bin.upper_bound {
            return bin.woe;
        }
    }
    // Last bin includes upper bound
    if let Some(last) = bins.last() {
        if value >= last.lower_bound {
            return last.woe;
        }
    }
    0.0 // Fallback
}

/// Calculate AUC using Mann-Whitney U statistic
fn calculate_auc(sorted_pairs: &[(f64, i32)]) -> f64 {
    let n = sorted_pairs.len();
    if n == 0 {
        return 0.5;
    }

    // Count positives and negatives
    let n_pos: usize = sorted_pairs.iter().filter(|(_, t)| *t == 1).count();
    let n_neg = n - n_pos;

    if n_pos == 0 || n_neg == 0 {
        return 0.5;
    }

    // Calculate sum of ranks for positive class
    // Using average rank for ties
    let mut rank_sum_pos = 0.0;
    let mut i = 0;

    while i < n {
        let current_value = sorted_pairs[i].0;
        let mut j = i;

        // Find all ties with same value
        while j < n && (sorted_pairs[j].0 - current_value).abs() < 1e-10 {
            j += 1;
        }

        // Average rank for this group (1-indexed)
        let avg_rank = (i + j + 1) as f64 / 2.0;

        // Add to sum for positive class members
        for k in i..j {
            if sorted_pairs[k].1 == 1 {
                rank_sum_pos += avg_rank;
            }
        }

        i = j;
    }

    // Mann-Whitney U statistic
    let u = rank_sum_pos - (n_pos as f64 * (n_pos as f64 + 1.0)) / 2.0;

    // AUC = U / (n_pos * n_neg)
    u / (n_pos as f64 * n_neg as f64)
}

/// Get list of features with Gini below the threshold
pub fn get_low_gini_features(analyses: &[IvAnalysis], threshold: f64) -> Vec<String> {
    analyses
        .iter()
        .filter(|a| a.gini.abs() < threshold)
        .map(|a| a.feature_name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_woe_iv_calculation() {
        // Test WoE/IV calculation with known values
        let (woe, iv) = calculate_woe_iv(10, 90, 100, 900);
        
        // With smoothing, dist_events ≈ 10.5/100.5, dist_non_events ≈ 90.5/900.5
        assert!(woe.abs() < 0.1, "WoE should be close to 0 for equal distributions");
        assert!(iv >= 0.0, "IV should be non-negative");
    }

    #[test]
    fn test_auc_calculation() {
        // Perfect separation: all 0s have lower values than all 1s
        let perfect = vec![(1.0, 0), (2.0, 0), (3.0, 1), (4.0, 1)];
        let auc = calculate_auc(&perfect);
        assert!((auc - 1.0).abs() < 0.01, "Perfect separation should give AUC ≈ 1.0");

        // Random: mixed values
        let random = vec![(1.0, 0), (2.0, 1), (3.0, 0), (4.0, 1)];
        let auc = calculate_auc(&random);
        assert!(auc > 0.4 && auc < 0.6, "Random should give AUC ≈ 0.5");
    }
}

