//! Solver-based optimal binning using Mixed Integer Programming (MIP)
//!
//! This module implements optimal binning using the HiGHS solver via good_lp.
//! It finds globally optimal bin boundaries that maximize Information Value (IV)
//! subject to constraints like bin count and optional monotonicity.

mod model;
mod monotonicity;
mod precompute;

use anyhow::Result;
use serde::Serialize;

use super::iv::WoeBin;

pub use monotonicity::MonotonicityConstraint;

/// Configuration for the solver-based optimal binning
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SolverConfig {
    /// Maximum time allowed for solver per feature (seconds)
    pub timeout_seconds: u64,
    /// MIP gap tolerance - solver stops when gap falls below this
    pub gap_tolerance: f64,
    /// Monotonicity constraint for WoE pattern
    pub monotonicity: MonotonicityConstraint,
    /// Minimum samples per bin
    pub min_bin_samples: usize,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            gap_tolerance: 0.01,
            monotonicity: MonotonicityConstraint::None,
            min_bin_samples: 5,
        }
    }
}

/// Result from the optimal binning solver
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SolverResult {
    /// Indices defining bin boundaries: each (start, end) pair indicates
    /// which prebins should be merged into a single final bin
    pub bin_boundaries: Vec<(usize, usize)>,
    /// Total IV achieved by this solution
    pub total_iv: f64,
    /// Time taken to solve (milliseconds)
    pub solve_time_ms: u64,
    /// MIP gap achieved (0.0 = optimal)
    pub gap: f64,
    /// The monotonicity constraint that was applied
    pub monotonicity_applied: MonotonicityConstraint,
}

/// Solve optimal binning for numeric features using MIP
///
/// Takes a vector of prebins and finds the optimal way to merge them
/// into `target_bins` final bins that maximizes total IV.
///
/// # Arguments
/// * `prebins` - Vector of pre-computed bins from quantile or CART prebinning
/// * `target_bins` - Desired number of final bins
/// * `config` - Solver configuration (timeout, gap tolerance, monotonicity)
/// * `total_events` - Total weighted event count
/// * `total_non_events` - Total weighted non-event count
/// * `total_samples` - Total weighted sample count
///
/// # Returns
/// SolverResult containing the optimal bin boundaries and statistics
pub fn solve_optimal_binning(
    prebins: &[WoeBin],
    target_bins: usize,
    config: &SolverConfig,
    total_events: f64,
    total_non_events: f64,
    total_samples: f64,
) -> Result<SolverResult> {
    model::solve_numeric_binning(
        prebins,
        target_bins,
        config,
        total_events,
        total_non_events,
        total_samples,
    )
}

/// Reconstruct final WoeBin vector from solver result
///
/// Takes the original prebins and the solver's bin boundary decisions,
/// merging prebins as specified to produce the final bins.
pub fn reconstruct_bins_from_solution(
    prebins: &[WoeBin],
    result: &SolverResult,
    total_events: f64,
    total_non_events: f64,
    total_samples: f64,
) -> Vec<WoeBin> {
    model::reconstruct_bins(
        prebins,
        result,
        total_events,
        total_non_events,
        total_samples,
    )
}

/// Category statistics for categorical binning
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct CategoryStats {
    pub category: String,
    pub events: f64,
    pub non_events: f64,
    pub count: f64,
    pub event_rate: f64,
}

/// Solve optimal binning for categorical features
///
/// Categories should be pre-sorted by event rate (ascending) before calling.
#[allow(dead_code)]
pub fn solve_categorical_optimal_binning(
    sorted_categories: &[CategoryStats],
    target_bins: usize,
    config: &SolverConfig,
    total_events: f64,
    total_non_events: f64,
    total_samples: f64,
) -> Result<SolverResult> {
    model::solve_categorical_binning(
        sorted_categories,
        target_bins,
        config,
        total_events,
        total_non_events,
        total_samples,
    )
}
