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

use super::target::{create_target_mask, TargetMapping};

/// Number of initial quantile pre-bins before merging
const PRE_BIN_COUNT: usize = 50;

/// Minimum samples per bin to avoid unstable WoE estimates
const MIN_BIN_SAMPLES: usize = 5;

/// Smoothing constant to avoid log(0) in WoE calculation (Laplace smoothing)
const SMOOTHING: f64 = 0.5;

/// Default minimum samples per category before merging into "OTHER"
const DEFAULT_MIN_CATEGORY_SAMPLES: usize = 5;

/// Binning strategy for pre-bin creation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum BinningStrategy {
    /// Equal-frequency binning (default) - bins have approximately equal sample counts
    #[default]
    Quantile,
    /// CART-style decision tree binning - splits maximize information gain
    Cart,
}

impl std::fmt::Display for BinningStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinningStrategy::Quantile => write!(f, "quantile"),
            BinningStrategy::Cart => write!(f, "cart"),
        }
    }
}

impl std::str::FromStr for BinningStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "quantile" => Ok(BinningStrategy::Quantile),
            "cart" => Ok(BinningStrategy::Cart),
            _ => Err(format!("Unknown binning strategy: '{}'. Use 'quantile' or 'cart'.", s)),
        }
    }
}

/// Feature type for IV analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FeatureType {
    Numeric,
    Categorical,
}

/// A single bin with WoE statistics for categorical features
#[derive(Debug, Clone, Serialize)]
pub struct CategoricalWoeBin {
    /// Category value (string)
    pub category: String,
    /// Count of events (target = 1) in this category
    pub events: usize,
    /// Count of non-events (target = 0) in this category
    pub non_events: usize,
    /// Weight of Evidence for this category
    pub woe: f64,
    /// Contribution to total IV from this category
    pub iv_contribution: f64,
    /// Total samples in this category
    pub count: usize,
    /// Percentage of total population in this category
    pub population_pct: f64,
    /// Event rate (events / count)
    pub event_rate: f64,
}

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
    /// Total samples in this bin
    pub count: usize,
    /// Percentage of total population in this bin
    pub population_pct: f64,
    /// Event rate (events / count)
    pub event_rate: f64,
}

/// A bin for missing/null values with WoE statistics
#[derive(Debug, Clone, Serialize)]
pub struct MissingBin {
    /// Count of events (target = 1) with missing feature values
    pub events: usize,
    /// Count of non-events (target = 0) with missing feature values
    pub non_events: usize,
    /// Weight of Evidence for missing values
    pub woe: f64,
    /// Contribution to total IV from missing values
    pub iv_contribution: f64,
    /// Total samples with missing values
    pub count: usize,
    /// Percentage of total population with missing values
    pub population_pct: f64,
    /// Event rate (events / count)
    pub event_rate: f64,
}

/// Complete IV analysis results for a single feature
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]  // Fields may be used for reporting/debugging
pub struct IvAnalysis {
    /// Name of the analyzed feature
    pub feature_name: String,
    /// Type of feature (Numeric or Categorical)
    pub feature_type: FeatureType,
    /// Bins with WoE statistics (for numeric features)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bins: Vec<WoeBin>,
    /// Categories with WoE statistics (for categorical features)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<CategoricalWoeBin>,
    /// Missing value bin (for features with null values)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_bin: Option<MissingBin>,
    /// Total Information Value
    pub iv: f64,
    /// Gini coefficient calculated on WoE-encoded values
    pub gini: f64,
}

// ============================================================================
// CART Binning Helper Functions
// ============================================================================

/// Calculate Gini impurity for a set of samples
/// 
/// Gini impurity measures the probability of incorrectly classifying a randomly
/// chosen element. For binary classification: Gini = 2 * p * (1 - p)
/// where p is the proportion of positive class (events).
fn gini_impurity(events: usize, non_events: usize) -> f64 {
    let total = (events + non_events) as f64;
    if total == 0.0 {
        return 0.0;
    }
    let p = events as f64 / total;
    2.0 * p * (1.0 - p)
}

/// Find the best split point that maximizes information gain (Gini reduction)
/// 
/// # Arguments
/// * `sorted_pairs` - Slice of (value, target) pairs, sorted by value
/// * `min_samples` - Minimum samples required on each side of the split
/// 
/// # Returns
/// Option of (split_index, information_gain) or None if no valid split found
fn find_best_split(
    sorted_pairs: &[(f64, i32)],
    min_samples: usize,
) -> Option<(usize, f64)> {
    let n = sorted_pairs.len();
    if n < 2 * min_samples {
        return None;
    }

    // Count total events and non-events
    let total_events: usize = sorted_pairs.iter().filter(|(_, t)| *t == 1).count();
    let total_non_events = n - total_events;
    
    // Calculate parent Gini impurity
    let parent_gini = gini_impurity(total_events, total_non_events);
    
    let mut best_gain = 0.0;
    let mut best_split_idx = None;
    
    // Track running counts for left side
    let mut left_events = 0usize;
    let mut left_non_events = 0usize;
    
    // Try each possible split point
    for i in 0..n - 1 {
        // Update left side counts
        if sorted_pairs[i].1 == 1 {
            left_events += 1;
        } else {
            left_non_events += 1;
        }
        
        let left_total = i + 1;
        let right_total = n - left_total;
        
        // Check minimum samples constraint
        if left_total < min_samples || right_total < min_samples {
            continue;
        }
        
        // Skip if this value equals the next (avoid splitting within same value)
        if (sorted_pairs[i].0 - sorted_pairs[i + 1].0).abs() < 1e-10 {
            continue;
        }
        
        // Calculate right side counts
        let right_events = total_events - left_events;
        let right_non_events = total_non_events - left_non_events;
        
        // Calculate weighted Gini for children
        let left_gini = gini_impurity(left_events, left_non_events);
        let right_gini = gini_impurity(right_events, right_non_events);
        
        let left_weight = left_total as f64 / n as f64;
        let right_weight = right_total as f64 / n as f64;
        
        let weighted_child_gini = left_weight * left_gini + right_weight * right_gini;
        
        // Information gain = parent_gini - weighted_child_gini
        let gain = parent_gini - weighted_child_gini;
        
        if gain > best_gain {
            best_gain = gain;
            best_split_idx = Some(i + 1); // Split index is where right side starts
        }
    }
    
    best_split_idx.map(|idx| (idx, best_gain))
}

/// Recursively find CART split points
/// 
/// # Arguments
/// * `sorted_pairs` - Slice of (value, target) pairs, sorted by value
/// * `max_splits` - Maximum number of splits allowed
/// * `min_samples` - Minimum samples per bin
/// * `split_indices` - Accumulator for split indices found
fn find_cart_splits_recursive(
    sorted_pairs: &[(f64, i32)],
    offset: usize,
    max_splits: usize,
    min_samples: usize,
    split_indices: &mut Vec<usize>,
) {
    if max_splits == 0 || sorted_pairs.len() < 2 * min_samples {
        return;
    }
    
    if let Some((local_split_idx, _gain)) = find_best_split(sorted_pairs, min_samples) {
        let global_split_idx = offset + local_split_idx;
        split_indices.push(global_split_idx);
        
        // Recursively split left and right partitions
        let (left, right) = sorted_pairs.split_at(local_split_idx);
        
        let remaining_splits = max_splits - 1;
        let left_splits = remaining_splits / 2;
        let right_splits = remaining_splits - left_splits;
        
        find_cart_splits_recursive(left, offset, left_splits, min_samples, split_indices);
        find_cart_splits_recursive(right, global_split_idx, right_splits, min_samples, split_indices);
    }
}

/// Create pre-bins using CART-style decision tree splits
/// 
/// Algorithm:
/// 1. Sort data by feature value
/// 2. Recursively find split points that maximize information gain
/// 3. Create bins from the split boundaries
fn create_cart_prebins(
    sorted_pairs: &[(f64, i32)],
    max_bins: usize,
    min_bin_samples: usize,
    total_events: usize,
    total_non_events: usize,
    total_samples: usize,
) -> Vec<WoeBin> {
    let n = sorted_pairs.len();
    
    // Maximum splits = max_bins - 1
    let max_splits = max_bins.saturating_sub(1);
    
    // Find split points recursively
    let mut split_indices = Vec::new();
    find_cart_splits_recursive(sorted_pairs, 0, max_splits, min_bin_samples, &mut split_indices);
    
    // Sort split indices
    split_indices.sort_unstable();
    
    // Create bins from split indices
    let mut bins = Vec::new();
    let mut start_idx = 0;
    
    for &split_idx in &split_indices {
        if split_idx > start_idx && split_idx <= n {
            let bin_pairs = &sorted_pairs[start_idx..split_idx];
            if let Some(bin) = create_woe_bin_from_pairs(
                bin_pairs,
                start_idx,
                split_idx,
                n,
                sorted_pairs,
                total_events,
                total_non_events,
                total_samples,
            ) {
                bins.push(bin);
            }
            start_idx = split_idx;
        }
    }
    
    // Create final bin
    if start_idx < n {
        let bin_pairs = &sorted_pairs[start_idx..];
        if let Some(bin) = create_woe_bin_from_pairs(
            bin_pairs,
            start_idx,
            n,
            n,
            sorted_pairs,
            total_events,
            total_non_events,
            total_samples,
        ) {
            bins.push(bin);
        }
    }
    
    // If no valid bins created, fall back to a single bin
    if bins.is_empty() {
        let events: usize = sorted_pairs.iter().filter(|(_, t)| *t == 1).count();
        let non_events = sorted_pairs.len() - events;
        let count = sorted_pairs.len();
        let (woe, iv_contrib) = calculate_woe_iv(events, non_events, total_events, total_non_events);
        
        bins.push(WoeBin {
            lower_bound: sorted_pairs.first().map(|(v, _)| *v).unwrap_or(f64::NEG_INFINITY),
            upper_bound: f64::INFINITY,
            events,
            non_events,
            woe,
            iv_contribution: iv_contrib,
            count,
            population_pct: count as f64 / total_samples as f64 * 100.0,
            event_rate: if count > 0 { events as f64 / count as f64 } else { 0.0 },
        });
    }
    
    bins
}

/// Create a WoeBin from a slice of pairs
fn create_woe_bin_from_pairs(
    bin_pairs: &[(f64, i32)],
    _start_idx: usize,
    end_idx: usize,
    total_len: usize,
    all_pairs: &[(f64, i32)],
    total_events: usize,
    total_non_events: usize,
    total_samples: usize,
) -> Option<WoeBin> {
    if bin_pairs.is_empty() {
        return None;
    }
    
    let lower = bin_pairs.first().map(|(v, _)| *v).unwrap_or(f64::NEG_INFINITY);
    let upper = if end_idx < total_len {
        all_pairs[end_idx].0
    } else {
        f64::INFINITY
    };
    
    let events: usize = bin_pairs.iter().filter(|(_, t)| *t == 1).count();
    let non_events = bin_pairs.len() - events;
    let count = bin_pairs.len();
    
    let (woe, iv_contrib) = calculate_woe_iv(events, non_events, total_events, total_non_events);
    
    Some(WoeBin {
        lower_bound: lower,
        upper_bound: upper,
        events,
        non_events,
        woe,
        iv_contribution: iv_contrib,
        count,
        population_pct: count as f64 / total_samples as f64 * 100.0,
        event_rate: if count > 0 { events as f64 / count as f64 } else { 0.0 },
    })
}

// ============================================================================
// Main Analysis Functions
// ============================================================================

/// Analyze all features (numeric and categorical) and calculate their IV
///
/// # Arguments
/// * `df` - Reference to the DataFrame (avoids re-collecting from LazyFrame)
/// * `target` - Name of the binary target column (must contain 0 and 1, or be mapped via target_mapping)
/// * `num_bins` - Target number of bins after merging
/// * `target_mapping` - Optional mapping for non-binary target columns
/// * `binning_strategy` - Strategy for creating initial bins (Quantile or Cart)
/// * `min_category_samples` - Minimum samples per category before merging into "OTHER"
///
/// # Returns
/// Vector of IvAnalysis for each feature, sorted by IV descending
pub fn analyze_features_iv(
    df: &DataFrame,
    target: &str,
    num_bins: usize,
    target_mapping: Option<&TargetMapping>,
    binning_strategy: BinningStrategy,
    min_category_samples: Option<usize>,
) -> Result<Vec<IvAnalysis>> {
    let min_cat_samples = min_category_samples.unwrap_or(DEFAULT_MIN_CATEGORY_SAMPLES);

    // Get target values based on whether we have a mapping
    let target_values: Vec<Option<i32>> = if let Some(mapping) = target_mapping {
        // Use the mapping to convert target values
        create_target_mask(df, target, mapping)?
    } else {
        // Validate binary target and get values directly
        validate_binary_target(df, target)?;
        
        let target_col = df.column(target)?;
        target_col
            .cast(&DataType::Int32)?
            .i32()?
            .into_iter()
            .collect()
    };

    // Get numeric columns (excluding target)
    let numeric_cols: Vec<String> = df
        .get_columns()
        .iter()
        .filter(|col| col.dtype().is_primitive_numeric() && col.name() != target)
        .map(|col| col.name().to_string())
        .collect();

    // Get categorical columns (String/Utf8 types, excluding target)
    let categorical_cols: Vec<String> = df
        .get_columns()
        .iter()
        .filter(|col| {
            matches!(col.dtype(), DataType::String | DataType::Categorical(_, _))
                && col.name() != target
        })
        .map(|col| col.name().to_string())
        .collect();

    let num_numeric = numeric_cols.len();
    let num_categorical = categorical_cols.len();
    let total_features = num_numeric + num_categorical;

    if total_features == 0 {
        return Ok(Vec::new());
    }

    // Create progress bar
    let pb = ProgressBar::new(total_features as u64);
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

    // Process numeric features in parallel
    let numeric_analyses: Vec<IvAnalysis> = numeric_cols
        .par_iter()
        .filter_map(|col_name| {
            let result = analyze_single_numeric_feature(
                df,
                col_name,
                &target_values,
                num_bins,
                binning_strategy,
            );

            // Update progress
            let count = progress_counter.fetch_add(1, Ordering::Relaxed);
            if count % 10 == 0 || count == (total_features as u64 - 1) {
                pb.set_position(count + 1);
            }

            match result {
                Ok(analysis) => Some(analysis),
                Err(_) => None, // Skip features that fail (e.g., all null)
            }
        })
        .collect();

    // Process categorical features in parallel
    let categorical_analyses: Vec<IvAnalysis> = categorical_cols
        .par_iter()
        .filter_map(|col_name| {
            let result = analyze_categorical_feature(df, col_name, &target_values, min_cat_samples);

            // Update progress
            let count = progress_counter.fetch_add(1, Ordering::Relaxed);
            if count % 10 == 0 || count == (total_features as u64 - 1) {
                pb.set_position(count + 1);
            }

            match result {
                Ok(analysis) => Some(analysis),
                Err(_) => None, // Skip features that fail
            }
        })
        .collect();

    pb.finish_with_message(format!(
        "   [OK] Analyzed {} features ({} numeric, {} categorical)",
        numeric_analyses.len() + categorical_analyses.len(),
        numeric_analyses.len(),
        categorical_analyses.len()
    ));

    // Combine and sort by IV descending
    let mut all_analyses: Vec<IvAnalysis> = numeric_analyses
        .into_iter()
        .chain(categorical_analyses)
        .collect();
    all_analyses.sort_by(|a, b| b.iv.partial_cmp(&a.iv).unwrap_or(std::cmp::Ordering::Equal));

    Ok(all_analyses)
}

/// Validate that the target column is binary (contains only 0 and 1)
///
/// This function handles edge cases from CSV/Parquet conversion:
/// - Empty or all-null columns
/// - Float64 columns with values like 0.0 and 1.0 (with tolerance)
/// - Integer columns with 0 and 1
fn validate_binary_target(df: &DataFrame, target: &str) -> Result<()> {
    let target_col = df
        .column(target)
        .with_context(|| format!("Target column '{}' not found", target))?;

    // Check for empty or all-null column first
    if target_col.len() == 0 {
        anyhow::bail!("Target column '{}' is empty", target);
    }

    if target_col.null_count() == target_col.len() {
        anyhow::bail!("Target column '{}' contains only null values", target);
    }

    // Cast to Float64 first to handle both integer and float types uniformly
    let float_col = target_col.cast(&DataType::Float64)?;
    let unique = float_col.unique()?;

    let unique_values: Vec<f64> = unique
        .f64()?
        .into_iter()
        .filter_map(|v| v) // Skip nulls
        .collect();

    if unique_values.is_empty() {
        anyhow::bail!("Target column '{}' has no valid (non-null) values", target);
    }

    // Check if values are 0.0 and 1.0 with tolerance for floating point precision
    const TOLERANCE: f64 = 1e-9;
    let valid = unique_values.len() <= 2
        && unique_values
            .iter()
            .all(|&v| (v - 0.0).abs() < TOLERANCE || (v - 1.0).abs() < TOLERANCE);

    if !valid {
        anyhow::bail!(
            "Target column '{}' must be binary (0/1). Found {} unique values: {:?}",
            target,
            unique_values.len(),
            unique_values
        );
    }

    Ok(())
}

/// Analyze a single numeric feature and calculate its IV
/// 
/// Missing feature values are placed in a dedicated MISSING bin rather than being dropped.
/// Only records with invalid/unmapped target values are excluded from the analysis.
fn analyze_single_numeric_feature(
    df: &DataFrame,
    col_name: &str,
    target_values: &[Option<i32>],
    num_bins: usize,
    binning_strategy: BinningStrategy,
) -> Result<IvAnalysis> {
    let col = df.column(col_name)?;
    let float_col = col.cast(&DataType::Float64)?;
    let values = float_col.f64()?;

    // Separate non-null value/target pairs and missing value targets
    // Only filter out records where target is None (not matching event or non-event in mapping)
    let mut pairs: Vec<(f64, i32)> = Vec::new();
    let mut missing_events: usize = 0;
    let mut missing_non_events: usize = 0;

    for (v, t) in values.iter().zip(target_values.iter()) {
        match (v, t) {
            (Some(val), Some(target)) => {
                // Non-null feature value with valid target
                pairs.push((val, *target));
            }
            (None, Some(target)) => {
                // Missing feature value with valid target -> goes to MISSING bin
                if *target == 1 {
                    missing_events += 1;
                } else {
                    missing_non_events += 1;
                }
            }
            (_, None) => {
                // Invalid/unmapped target -> skip this record entirely
            }
        }
    }

    let missing_count = missing_events + missing_non_events;
    let total_valid_records = pairs.len() + missing_count;

    // Need at least some valid records to proceed
    if total_valid_records < MIN_BIN_SAMPLES {
        anyhow::bail!("Insufficient valid records for feature '{}'", col_name);
    }

    // Count total events and non-events (including missing bin)
    let non_missing_events: usize = pairs.iter().filter(|(_, t)| *t == 1).count();
    let non_missing_non_events: usize = pairs.len() - non_missing_events;
    
    let total_events = non_missing_events + missing_events;
    let total_non_events = non_missing_non_events + missing_non_events;
    let total_samples = total_valid_records;

    if total_events == 0 || total_non_events == 0 {
        anyhow::bail!(
            "Feature '{}' has no variation in target (all 0s or all 1s)",
            col_name
        );
    }

    // Create MISSING bin if there are missing values
    let missing_bin = if missing_count > 0 {
        let (woe, iv_contrib) = calculate_woe_iv(missing_events, missing_non_events, total_events, total_non_events);
        Some(MissingBin {
            events: missing_events,
            non_events: missing_non_events,
            woe,
            iv_contribution: iv_contrib,
            count: missing_count,
            population_pct: missing_count as f64 / total_samples as f64 * 100.0,
            event_rate: if missing_count > 0 { missing_events as f64 / missing_count as f64 } else { 0.0 },
        })
    } else {
        None
    };

    // If all values are missing or too few non-missing values for binning,
    // return early with just the missing bin
    if pairs.len() < MIN_BIN_SAMPLES * 2 {
        let iv = missing_bin.as_ref().map(|b| b.iv_contribution).unwrap_or(0.0);
        // With only missing bin and insufficient non-missing values for binning,
        // Gini is 0 as there's no discrimination possible
        let gini = 0.0;
        
        return Ok(IvAnalysis {
            feature_name: col_name.to_string(),
            feature_type: FeatureType::Numeric,
            bins: Vec::new(),
            categories: Vec::new(),
            missing_bin,
            iv,
            gini,
        });
    }

    // Sort by value for binning
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Phase 1: Create initial pre-bins based on strategy (for non-missing values)
    let pre_bins = match binning_strategy {
        BinningStrategy::Quantile => {
            create_quantile_prebins(&pairs, PRE_BIN_COUNT, total_events, total_non_events, total_samples)
        }
        BinningStrategy::Cart => {
            create_cart_prebins(&pairs, num_bins, MIN_BIN_SAMPLES, total_events, total_non_events, total_samples)
        }
    };

    // Phase 2: Greedy merge until target bin count (only needed for Quantile strategy)
    let final_bins = match binning_strategy {
        BinningStrategy::Quantile => {
            greedy_merge_bins(pre_bins, num_bins, total_events, total_non_events, total_samples)
        }
        BinningStrategy::Cart => {
            // CART already produces the target number of bins
            pre_bins
        }
    };

    // Calculate total IV (including missing bin contribution)
    let bins_iv: f64 = final_bins.iter().map(|b| b.iv_contribution).sum();
    let missing_iv: f64 = missing_bin.as_ref().map(|b| b.iv_contribution).unwrap_or(0.0);
    let iv = bins_iv + missing_iv;

    // Calculate Gini on WoE-encoded values (including missing bin)
    let gini = calculate_gini_on_woe_with_missing(&pairs, &final_bins, &missing_bin, missing_events, missing_non_events);

    Ok(IvAnalysis {
        feature_name: col_name.to_string(),
        feature_type: FeatureType::Numeric,
        bins: final_bins,
        categories: Vec::new(),
        missing_bin,
        iv,
        gini,
    })
}

/// Analyze a categorical feature and calculate its IV
/// 
/// Missing feature values are placed in a dedicated MISSING bin rather than being dropped.
/// Only records with invalid/unmapped target values are excluded from the analysis.
fn analyze_categorical_feature(
    df: &DataFrame,
    col_name: &str,
    target_values: &[Option<i32>],
    min_category_samples: usize,
) -> Result<IvAnalysis> {
    let col = df.column(col_name)?;
    
    // Get string values
    let string_col = col.cast(&DataType::String)?;
    let values = string_col.str()?;

    // Collect category/target pairs with counts, including MISSING for null values
    let mut category_stats: std::collections::HashMap<String, (usize, usize)> = std::collections::HashMap::new();
    let mut missing_events: usize = 0;
    let mut missing_non_events: usize = 0;
    
    for (val, target) in values.iter().zip(target_values.iter()) {
        match (val, target) {
            (Some(cat), Some(t)) => {
                // Non-null category with valid target
                let entry = category_stats.entry(cat.to_string()).or_insert((0, 0));
                if *t == 1 {
                    entry.0 += 1; // events
                } else {
                    entry.1 += 1; // non_events
                }
            }
            (None, Some(t)) => {
                // Missing category value with valid target -> goes to MISSING bin
                if *t == 1 {
                    missing_events += 1;
                } else {
                    missing_non_events += 1;
                }
            }
            (_, None) => {
                // Invalid/unmapped target -> skip this record entirely
            }
        }
    }

    let missing_count = missing_events + missing_non_events;
    let category_total: usize = category_stats.values().map(|(e, ne)| e + ne).sum();
    let total_valid_records = category_total + missing_count;

    if total_valid_records == 0 {
        anyhow::bail!("No valid records found for feature '{}'", col_name);
    }

    // Calculate totals (including missing)
    let cat_events: usize = category_stats.values().map(|(e, _)| *e).sum();
    let cat_non_events: usize = category_stats.values().map(|(_, ne)| *ne).sum();
    
    let total_events = cat_events + missing_events;
    let total_non_events = cat_non_events + missing_non_events;
    let total_samples = total_valid_records;

    if total_events == 0 || total_non_events == 0 {
        anyhow::bail!(
            "Feature '{}' has no variation in target (all 0s or all 1s)",
            col_name
        );
    }

    // Create MISSING bin if there are missing values
    let missing_bin = if missing_count > 0 {
        let (woe, iv_contrib) = calculate_woe_iv(missing_events, missing_non_events, total_events, total_non_events);
        Some(MissingBin {
            events: missing_events,
            non_events: missing_non_events,
            woe,
            iv_contribution: iv_contrib,
            count: missing_count,
            population_pct: missing_count as f64 / total_samples as f64 * 100.0,
            event_rate: if missing_count > 0 { missing_events as f64 / missing_count as f64 } else { 0.0 },
        })
    } else {
        None
    };

    // Merge rare categories into "OTHER"
    let mut other_events = 0usize;
    let mut other_non_events = 0usize;
    let mut final_categories: Vec<(String, usize, usize)> = Vec::new();

    for (cat, (events, non_events)) in category_stats {
        let count = events + non_events;
        if count < min_category_samples {
            other_events += events;
            other_non_events += non_events;
        } else {
            final_categories.push((cat, events, non_events));
        }
    }

    // Add "OTHER" category if there are merged categories
    if other_events + other_non_events > 0 {
        final_categories.push(("OTHER".to_string(), other_events, other_non_events));
    }

    // Create CategoricalWoeBin for each category
    let mut categories: Vec<CategoricalWoeBin> = final_categories
        .into_iter()
        .map(|(category, events, non_events)| {
            let count = events + non_events;
            let (woe, iv_contribution) = calculate_woe_iv(events, non_events, total_events, total_non_events);
            
            CategoricalWoeBin {
                category,
                events,
                non_events,
                woe,
                iv_contribution,
                count,
                population_pct: count as f64 / total_samples as f64 * 100.0,
                event_rate: if count > 0 { events as f64 / count as f64 } else { 0.0 },
            }
        })
        .collect();

    // Sort by WoE
    categories.sort_by(|a, b| a.woe.partial_cmp(&b.woe).unwrap_or(std::cmp::Ordering::Equal));

    // Calculate total IV (including missing bin contribution)
    let categories_iv: f64 = categories.iter().map(|c| c.iv_contribution).sum();
    let missing_iv: f64 = missing_bin.as_ref().map(|b| b.iv_contribution).unwrap_or(0.0);
    let iv = categories_iv + missing_iv;

    // Calculate Gini using category WoE values (including missing bin)
    let gini = calculate_gini_on_categories_with_missing(&categories, &missing_bin, total_events, total_non_events);

    Ok(IvAnalysis {
        feature_name: col_name.to_string(),
        feature_type: FeatureType::Categorical,
        bins: Vec::new(),
        categories,
        missing_bin,
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
    total_samples: usize,
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
        let count = bin_pairs.len();

        let (woe, iv_contrib) =
            calculate_woe_iv(events, non_events, total_events, total_non_events);

        bins.push(WoeBin {
            lower_bound: lower,
            upper_bound: upper,
            events,
            non_events,
            woe,
            iv_contribution: iv_contrib,
            count,
            population_pct: count as f64 / total_samples as f64 * 100.0,
            event_rate: if count > 0 { events as f64 / count as f64 } else { 0.0 },
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
    total_samples: usize,
) -> Vec<WoeBin> {
    while bins.len() > target_bins && bins.len() > 1 {
        // Find the adjacent pair whose merge results in minimum IV loss
        let mut min_loss = f64::MAX;
        let mut merge_idx = 0;

        for i in 0..bins.len() - 1 {
            let merged = merge_two_bins(&bins[i], &bins[i + 1], total_events, total_non_events, total_samples);
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
            total_samples,
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
    total_samples: usize,
) -> WoeBin {
    let events = bin1.events + bin2.events;
    let non_events = bin1.non_events + bin2.non_events;
    let count = bin1.count + bin2.count;
    let (woe, iv_contrib) = calculate_woe_iv(events, non_events, total_events, total_non_events);

    WoeBin {
        lower_bound: bin1.lower_bound,
        upper_bound: bin2.upper_bound,
        events,
        non_events,
        woe,
        iv_contribution: iv_contrib,
        count,
        population_pct: count as f64 / total_samples as f64 * 100.0,
        event_rate: if count > 0 { events as f64 / count as f64 } else { 0.0 },
    }
}

/// Calculate Gini coefficient on WoE-encoded values including missing bin
fn calculate_gini_on_woe_with_missing(
    sorted_pairs: &[(f64, i32)],
    bins: &[WoeBin],
    missing_bin: &Option<MissingBin>,
    missing_events: usize,
    missing_non_events: usize,
) -> f64 {
    // Encode each non-missing value with its bin's WoE
    let mut woe_target_pairs: Vec<(f64, i32)> = sorted_pairs
        .iter()
        .map(|(val, target)| {
            let woe = find_woe_for_value(*val, bins);
            (woe, *target)
        })
        .collect();

    // Add missing bin samples with their WoE
    if let Some(mb) = missing_bin {
        // Add events from missing bin
        for _ in 0..missing_events {
            woe_target_pairs.push((mb.woe, 1));
        }
        // Add non-events from missing bin
        for _ in 0..missing_non_events {
            woe_target_pairs.push((mb.woe, 0));
        }
    }

    if woe_target_pairs.is_empty() {
        return 0.0;
    }

    // Sort by WoE for AUC calculation
    woe_target_pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Calculate AUC using Mann-Whitney U statistic
    let auc = calculate_auc(&woe_target_pairs);

    // Gini = 2 * AUC - 1
    2.0 * auc - 1.0
}

/// Calculate Gini coefficient for categorical features including missing bin
fn calculate_gini_on_categories_with_missing(
    categories: &[CategoricalWoeBin],
    missing_bin: &Option<MissingBin>,
    total_events: usize,
    total_non_events: usize,
) -> f64 {
    if (categories.is_empty() && missing_bin.is_none()) || total_events == 0 || total_non_events == 0 {
        return 0.0;
    }

    // Create pairs of (WoE, target) for all samples
    let mut woe_target_pairs: Vec<(f64, i32)> = Vec::new();
    
    // Add category samples
    for cat in categories {
        // Add events (target = 1)
        for _ in 0..cat.events {
            woe_target_pairs.push((cat.woe, 1));
        }
        // Add non-events (target = 0)
        for _ in 0..cat.non_events {
            woe_target_pairs.push((cat.woe, 0));
        }
    }

    // Add missing bin samples
    if let Some(mb) = missing_bin {
        for _ in 0..mb.events {
            woe_target_pairs.push((mb.woe, 1));
        }
        for _ in 0..mb.non_events {
            woe_target_pairs.push((mb.woe, 0));
        }
    }

    if woe_target_pairs.is_empty() {
        return 0.0;
    }

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

        // No discrimination: 0s and 1s have same values (ties)
        let no_disc = vec![(1.0, 0), (1.0, 1), (2.0, 0), (2.0, 1)];
        let auc = calculate_auc(&no_disc);
        assert!(
            (auc - 0.5).abs() < 0.01, 
            "No discrimination should give AUC ≈ 0.5, got {}", 
            auc
        );
        
        // Partial discrimination: alternating pattern
        let partial = vec![(1.0, 0), (2.0, 1), (3.0, 0), (4.0, 1)];
        let auc = calculate_auc(&partial);
        assert!(
            auc > 0.5 && auc < 1.0, 
            "Partial discrimination should give AUC between 0.5 and 1.0, got {}", 
            auc
        );
    }

    #[test]
    fn test_validate_binary_target_valid_int() {
        // Valid binary target with integers
        let df = df! {
            "target" => [0i32, 1, 0, 1, 0, 1],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0],
        }.unwrap();
        
        assert!(validate_binary_target(&df, "target").is_ok());
    }

    #[test]
    fn test_validate_binary_target_valid_float() {
        // Valid binary target stored as floats (0.0 and 1.0)
        let df = df! {
            "target" => [0.0f64, 1.0, 0.0, 1.0, 0.0, 1.0],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0],
        }.unwrap();
        
        assert!(validate_binary_target(&df, "target").is_ok());
    }

    #[test]
    fn test_validate_binary_target_empty_column() {
        // Empty target column should fail
        let df = df! {
            "target" => Vec::<i32>::new(),
            "feature" => Vec::<f64>::new(),
        }.unwrap();
        
        let result = validate_binary_target(&df, "target");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_validate_binary_target_all_nulls() {
        // All-null target column should fail
        let df = df! {
            "target" => [None::<i32>, None, None, None],
            "feature" => [1.0f64, 2.0, 3.0, 4.0],
        }.unwrap();
        
        let result = validate_binary_target(&df, "target");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null"));
    }

    #[test]
    fn test_validate_binary_target_non_binary() {
        // Non-binary target (0, 1, 2) should fail
        let df = df! {
            "target" => [0i32, 1, 2, 0, 1, 2],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0],
        }.unwrap();
        
        let result = validate_binary_target(&df, "target");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be binary"));
    }

    #[test]
    fn test_validate_binary_target_column_not_found() {
        // Missing target column should fail
        let df = df! {
            "other" => [0i32, 1, 0, 1],
            "feature" => [1.0f64, 2.0, 3.0, 4.0],
        }.unwrap();
        
        let result = validate_binary_target(&df, "target");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_validate_binary_target_with_nulls_but_valid() {
        // Target with some nulls but valid 0/1 values should pass
        let df = df! {
            "target" => [Some(0i32), Some(1), None, Some(0), Some(1)],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        }.unwrap();
        
        assert!(validate_binary_target(&df, "target").is_ok());
    }

    #[test]
    fn test_gini_impurity() {
        // Pure node (all one class) should have 0 impurity
        assert!((gini_impurity(0, 10) - 0.0).abs() < 0.01);
        assert!((gini_impurity(10, 0) - 0.0).abs() < 0.01);
        
        // 50/50 split should have maximum impurity (0.5)
        assert!((gini_impurity(5, 5) - 0.5).abs() < 0.01);
        
        // Skewed splits should have lower impurity
        let skewed = gini_impurity(9, 1);
        assert!(skewed < 0.5, "Skewed split should have lower impurity than 50/50");
        assert!(skewed > 0.0, "Non-pure split should have positive impurity");
    }

    #[test]
    fn test_find_best_split() {
        // Perfect separation: all 0s below, all 1s above
        let pairs = vec![(1.0, 0), (2.0, 0), (3.0, 1), (4.0, 1)];
        
        let result = find_best_split(&pairs, 1);
        assert!(result.is_some(), "Should find a split");
        
        let (split_idx, gain) = result.unwrap();
        assert_eq!(split_idx, 2, "Should split between 2.0 and 3.0");
        assert!(gain > 0.0, "Should have positive gain");
    }

    #[test]
    fn test_find_best_split_no_valid_split() {
        // Too few samples for minimum constraint
        let pairs = vec![(1.0, 0), (2.0, 1)];
        
        let result = find_best_split(&pairs, 2); // min_samples = 2
        assert!(result.is_none(), "Should not find a split with insufficient samples");
    }

    #[test]
    fn test_binning_strategy_from_str() {
        assert_eq!("quantile".parse::<BinningStrategy>().unwrap(), BinningStrategy::Quantile);
        assert_eq!("cart".parse::<BinningStrategy>().unwrap(), BinningStrategy::Cart);
        assert_eq!("CART".parse::<BinningStrategy>().unwrap(), BinningStrategy::Cart);
        assert!("invalid".parse::<BinningStrategy>().is_err());
    }

    #[test]
    fn test_binning_strategy_display() {
        assert_eq!(BinningStrategy::Quantile.to_string(), "quantile");
        assert_eq!(BinningStrategy::Cart.to_string(), "cart");
    }

    #[test]
    fn test_cart_prebins_creates_valid_bins() {
        // Create sorted pairs with clear split point
        let pairs: Vec<(f64, i32)> = (0..20)
            .map(|i| {
                let val = i as f64;
                let target = if i < 10 { 0 } else { 1 };
                (val, target)
            })
            .collect();
        
        let bins = create_cart_prebins(&pairs, 3, 2, 10, 10, 20);
        
        assert!(!bins.is_empty(), "Should create at least one bin");
        assert!(bins.len() <= 3, "Should not exceed max bins");
        
        // Verify all samples are covered
        let total_count: usize = bins.iter().map(|b| b.count).sum();
        assert_eq!(total_count, 20, "All samples should be binned");
    }

    #[test]
    fn test_categorical_woe_bin_creation() {
        // Test that categorical analysis correctly groups and calculates
        let df = df! {
            "target" => [0i32, 0, 1, 1, 0, 0, 1, 1, 0, 1],
            "category" => ["A", "A", "A", "B", "B", "C", "C", "C", "C", "C"],
        }.unwrap();
        
        let target_values: Vec<Option<i32>> = vec![
            Some(0), Some(0), Some(1), Some(1), Some(0),
            Some(0), Some(1), Some(1), Some(0), Some(1)
        ];
        
        let result = analyze_categorical_feature(&df, "category", &target_values, 1);
        assert!(result.is_ok(), "Should analyze categorical feature");
        
        let analysis = result.unwrap();
        assert_eq!(analysis.feature_type, FeatureType::Categorical);
        assert!(!analysis.categories.is_empty(), "Should have category bins");
        assert!(analysis.bins.is_empty(), "Should not have numeric bins");
        
        // Check IV is positive
        assert!(analysis.iv >= 0.0, "IV should be non-negative");
    }

    #[test]
    fn test_woebin_enhanced_fields() {
        // Create a simple bin and verify enhanced fields
        let bins = create_quantile_prebins(
            &[(1.0, 0), (2.0, 0), (3.0, 1), (4.0, 1)],
            2,  // 2 pre-bins
            2,  // total_events
            2,  // total_non_events
            4,  // total_samples
        );
        
        for bin in &bins {
            assert!(bin.count > 0, "Bin count should be positive");
            assert!(bin.population_pct >= 0.0 && bin.population_pct <= 100.0, 
                "Population percent should be 0-100");
            assert!(bin.event_rate >= 0.0 && bin.event_rate <= 1.0,
                "Event rate should be 0-1");
        }
    }

    // =========================================================================
    // Tests for Missing Value Handling
    // =========================================================================

    #[test]
    fn test_numeric_feature_with_missing_values() {
        // Test that numeric feature analysis creates a MISSING bin for null values
        let df = df! {
            "target" => [0i32, 0, 1, 1, 0, 1, 0, 1, 0, 1],
            "feature" => [Some(1.0f64), Some(2.0), None, Some(4.0), None, Some(6.0), Some(7.0), Some(8.0), Some(9.0), Some(10.0)],
        }.unwrap();
        
        let target_values: Vec<Option<i32>> = vec![
            Some(0), Some(0), Some(1), Some(1), Some(0),
            Some(1), Some(0), Some(1), Some(0), Some(1)
        ];
        
        let result = analyze_single_numeric_feature(&df, "feature", &target_values, 5, BinningStrategy::Quantile);
        assert!(result.is_ok(), "Should analyze numeric feature with missing values");
        
        let analysis = result.unwrap();
        
        // Should have a missing bin
        assert!(analysis.missing_bin.is_some(), "Should have a MISSING bin for null values");
        
        let missing_bin = analysis.missing_bin.unwrap();
        assert_eq!(missing_bin.count, 2, "Missing bin should contain 2 samples");
        assert_eq!(missing_bin.events, 1, "Missing bin should have 1 event");
        assert_eq!(missing_bin.non_events, 1, "Missing bin should have 1 non-event");
        assert!(missing_bin.population_pct > 0.0, "Missing bin should have positive population percentage");
        
        // IV should include missing bin contribution
        assert!(analysis.iv >= 0.0, "IV should be non-negative");
    }

    #[test]
    fn test_categorical_feature_with_missing_values() {
        // Test that categorical feature analysis creates a MISSING bin for null values
        let df = df! {
            "target" => [0i32, 0, 1, 1, 0, 1, 0, 1, 0, 1],
            "category" => [Some("A"), Some("A"), None, Some("B"), None, Some("B"), Some("C"), Some("C"), Some("C"), Some("C")],
        }.unwrap();
        
        let target_values: Vec<Option<i32>> = vec![
            Some(0), Some(0), Some(1), Some(1), Some(0),
            Some(1), Some(0), Some(1), Some(0), Some(1)
        ];
        
        let result = analyze_categorical_feature(&df, "category", &target_values, 1);
        assert!(result.is_ok(), "Should analyze categorical feature with missing values");
        
        let analysis = result.unwrap();
        
        // Should have a missing bin
        assert!(analysis.missing_bin.is_some(), "Should have a MISSING bin for null values");
        
        let missing_bin = analysis.missing_bin.unwrap();
        assert_eq!(missing_bin.count, 2, "Missing bin should contain 2 samples");
        assert_eq!(missing_bin.events, 1, "Missing bin should have 1 event");
        assert_eq!(missing_bin.non_events, 1, "Missing bin should have 1 non-event");
        
        // IV should include missing bin contribution
        assert!(analysis.iv >= 0.0, "IV should be non-negative");
    }

    #[test]
    fn test_numeric_feature_no_missing_values() {
        // Test that numeric feature without missing values has no MISSING bin
        let df = df! {
            "target" => [0i32, 0, 1, 1, 0, 1, 0, 1, 0, 1],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        }.unwrap();
        
        let target_values: Vec<Option<i32>> = vec![
            Some(0), Some(0), Some(1), Some(1), Some(0),
            Some(1), Some(0), Some(1), Some(0), Some(1)
        ];
        
        let result = analyze_single_numeric_feature(&df, "feature", &target_values, 5, BinningStrategy::Quantile);
        assert!(result.is_ok(), "Should analyze numeric feature without missing values");
        
        let analysis = result.unwrap();
        
        // Should NOT have a missing bin
        assert!(analysis.missing_bin.is_none(), "Should not have MISSING bin when no null values");
    }

    #[test]
    fn test_only_drops_records_with_invalid_target() {
        // Test that only records with invalid target (None) are dropped, not missing feature values
        let df = df! {
            "target" => [0i32, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1],
            "feature" => [Some(1.0f64), Some(2.0), None, Some(4.0), None, Some(6.0), Some(7.0), Some(8.0), Some(9.0), Some(10.0), Some(11.0), Some(12.0)],
        }.unwrap();
        
        // Two records have None target (these should be dropped)
        // Two records have None feature (these should go to MISSING bin)
        let target_values: Vec<Option<i32>> = vec![
            Some(0), Some(0), Some(1), None, Some(0),  // 4th record has invalid target
            Some(1), Some(0), Some(1), None, Some(1),  // 9th record has invalid target
            Some(0), Some(1)
        ];
        
        let result = analyze_single_numeric_feature(&df, "feature", &target_values, 5, BinningStrategy::Quantile);
        assert!(result.is_ok(), "Should analyze feature");
        
        let analysis = result.unwrap();
        
        // Should have a missing bin (only for records where feature is null AND target is valid)
        assert!(analysis.missing_bin.is_some(), "Should have MISSING bin");
        
        let missing_bin = analysis.missing_bin.unwrap();
        // Only the 3rd row (target=Some(1)) and 5th row (target=Some(0)) have null features with valid targets
        assert_eq!(missing_bin.count, 2, "Missing bin should contain 2 samples (from rows with valid targets)");
    }

    #[test]
    fn test_all_missing_feature_values() {
        // Test feature where all values are missing (but targets are valid)
        let df = df! {
            "target" => [0i32, 0, 1, 1, 0, 1],
            "feature" => [None::<f64>, None, None, None, None, None],
        }.unwrap();
        
        let target_values: Vec<Option<i32>> = vec![
            Some(0), Some(0), Some(1), Some(1), Some(0), Some(1)
        ];
        
        let result = analyze_single_numeric_feature(&df, "feature", &target_values, 5, BinningStrategy::Quantile);
        assert!(result.is_ok(), "Should handle all-missing feature values");
        
        let analysis = result.unwrap();
        
        // Should only have missing bin, no regular bins
        assert!(analysis.missing_bin.is_some(), "Should have MISSING bin");
        assert!(analysis.bins.is_empty(), "Should have no regular bins");
        
        let missing_bin = analysis.missing_bin.unwrap();
        assert_eq!(missing_bin.count, 6, "Missing bin should contain all 6 samples");
        assert_eq!(missing_bin.events, 3, "Missing bin should have 3 events");
        assert_eq!(missing_bin.non_events, 3, "Missing bin should have 3 non-events");
    }

    #[test]
    fn test_missing_bin_woe_calculation() {
        // Test that MISSING bin WoE is calculated correctly
        let df = df! {
            "target" => [0i32, 0, 1, 1, 0, 1, 0, 1, 1, 1],
            "feature" => [Some(1.0f64), Some(2.0), None, None, None, Some(6.0), Some(7.0), Some(8.0), Some(9.0), Some(10.0)],
        }.unwrap();
        
        // Missing feature values: rows 3, 4, 5 with targets 1, 1, 0
        // So missing_events = 2, missing_non_events = 1
        let target_values: Vec<Option<i32>> = vec![
            Some(0), Some(0), Some(1), Some(1), Some(0),
            Some(1), Some(0), Some(1), Some(1), Some(1)
        ];
        
        let result = analyze_single_numeric_feature(&df, "feature", &target_values, 5, BinningStrategy::Quantile);
        assert!(result.is_ok(), "Should analyze feature");
        
        let analysis = result.unwrap();
        assert!(analysis.missing_bin.is_some(), "Should have MISSING bin");
        
        let missing_bin = analysis.missing_bin.unwrap();
        assert_eq!(missing_bin.events, 2, "Missing bin should have 2 events");
        assert_eq!(missing_bin.non_events, 1, "Missing bin should have 1 non-event");
        assert!(missing_bin.iv_contribution >= 0.0, "IV contribution should be non-negative");
        
        // WoE should reflect higher event rate in missing bin
        // (WoE sign depends on overall event/non-event distribution)
    }

    #[test]
    fn test_gini_includes_missing_bin() {
        // Test that Gini calculation includes samples from MISSING bin
        let df = df! {
            "target" => [0i32, 0, 1, 1, 0, 1, 0, 1, 0, 1],
            "feature" => [Some(1.0f64), Some(2.0), None, Some(4.0), None, Some(6.0), Some(7.0), Some(8.0), Some(9.0), Some(10.0)],
        }.unwrap();
        
        let target_values: Vec<Option<i32>> = vec![
            Some(0), Some(0), Some(1), Some(1), Some(0),
            Some(1), Some(0), Some(1), Some(0), Some(1)
        ];
        
        let result = analyze_single_numeric_feature(&df, "feature", &target_values, 5, BinningStrategy::Quantile);
        assert!(result.is_ok(), "Should analyze feature");
        
        let analysis = result.unwrap();
        
        // Gini should be calculated including missing bin
        // It should be in valid range [-1, 1]
        assert!(analysis.gini >= -1.0 && analysis.gini <= 1.0, 
            "Gini should be in valid range, got {}", analysis.gini);
    }
}

