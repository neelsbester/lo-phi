//! MIP model construction and solving for optimal binning
//!
//! This module builds and solves Mixed Integer Programming models
//! to find globally optimal bin boundaries.

use std::time::Instant;

use anyhow::{Context, Result};
use good_lp::{constraint, default_solver, variable, Expression, ProblemVariables, Solution, SolverModel, Variable};

use super::super::iv::WoeBin;
use super::monotonicity::MonotonicityConstraint;
use super::precompute::{get_precomputed_bin, precompute_categorical_iv_matrix, precompute_iv_matrix, PrecomputedBin};
use super::{CategoryStats, SolverConfig, SolverResult};

/// Smoothing constant for WoE calculation
const SMOOTHING: f64 = 0.5;

/// Solve the optimal binning problem for numeric features
pub fn solve_numeric_binning(
    prebins: &[WoeBin],
    target_bins: usize,
    config: &SolverConfig,
    total_events: f64,
    total_non_events: f64,
    total_samples: f64,
) -> Result<SolverResult> {
    let start_time = Instant::now();
    let n = prebins.len();

    // Edge case: if prebins <= target_bins, no merging needed
    if n <= target_bins {
        let boundaries: Vec<(usize, usize)> = (0..n).map(|i| (i, i)).collect();
        let total_iv: f64 = prebins.iter().map(|b| b.iv_contribution).sum();
        return Ok(SolverResult {
            bin_boundaries: boundaries,
            total_iv,
            solve_time_ms: start_time.elapsed().as_millis() as u64,
            gap: 0.0,
            monotonicity_applied: MonotonicityConstraint::None,
        });
    }

    // Precompute IV for all possible bin combinations
    let iv_matrix = precompute_iv_matrix(prebins, total_events, total_non_events);

    // Handle Auto monotonicity by trying all patterns and selecting best
    if config.monotonicity == MonotonicityConstraint::Auto {
        return solve_with_auto_monotonicity(
            prebins,
            target_bins,
            config,
            &iv_matrix,
            total_events,
            total_non_events,
            total_samples,
            start_time,
        );
    }

    // Solve with the specified monotonicity constraint
    solve_with_monotonicity(
        prebins,
        target_bins,
        config,
        &iv_matrix,
        config.monotonicity,
        total_events,
        total_non_events,
        total_samples,
        start_time,
    )
}

/// Solve with automatic monotonicity detection
fn solve_with_auto_monotonicity(
    prebins: &[WoeBin],
    target_bins: usize,
    config: &SolverConfig,
    iv_matrix: &[Vec<PrecomputedBin>],
    total_events: f64,
    total_non_events: f64,
    total_samples: f64,
    start_time: Instant,
) -> Result<SolverResult> {
    let patterns = [
        MonotonicityConstraint::None,
        MonotonicityConstraint::Ascending,
        MonotonicityConstraint::Descending,
        MonotonicityConstraint::Peak,
        MonotonicityConstraint::Valley,
    ];

    let mut best_result: Option<SolverResult> = None;

    for pattern in patterns {
        let result = solve_with_monotonicity(
            prebins,
            target_bins,
            config,
            iv_matrix,
            pattern,
            total_events,
            total_non_events,
            total_samples,
            start_time,
        );

        if let Ok(res) = result {
            if best_result.is_none() || res.total_iv > best_result.as_ref().unwrap().total_iv {
                best_result = Some(res);
            }
        }
    }

    best_result.context("No valid solution found with any monotonicity pattern")
}

/// Solve the MIP model with a specific monotonicity constraint
fn solve_with_monotonicity(
    prebins: &[WoeBin],
    target_bins: usize,
    config: &SolverConfig,
    iv_matrix: &[Vec<PrecomputedBin>],
    monotonicity: MonotonicityConstraint,
    _total_events: f64,
    _total_non_events: f64,
    _total_samples: f64,
    start_time: Instant,
) -> Result<SolverResult> {
    let n = prebins.len();
    let k = target_bins;

    let mut vars = ProblemVariables::new();

    // Objective: maximize total IV
    // We need to express IV in terms of cut variables
    // This is complex with cut-point formulation, so we'll use a different approach:
    // Use bin assignment variables z[i][j] = 1 if prebins i..=j form a bin

    // Actually, for simplicity, let's use the interval formulation
    // z[i][j] = 1 if prebins i through j are merged into one bin

    let mut z: Vec<Vec<Option<Variable>>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(n - i);
        for j in i..n {
            let bin = get_precomputed_bin(iv_matrix, i, j);
            // Only create variable if bin meets minimum sample requirement
            if bin.count >= config.min_bin_samples as f64 {
                row.push(Some(vars.add(variable().binary())));
            } else {
                row.push(None);
            }
        }
        z.push(row);
    }

    // Build objective: sum of IV contributions for selected bins
    let mut objective_terms: Vec<Expression> = Vec::new();
    for i in 0..n {
        for j in i..n {
            if let Some(var) = z[i][j - i] {
                let bin = get_precomputed_bin(iv_matrix, i, j);
                objective_terms.push(bin.iv * var);
            }
        }
    }

    let objective: Expression = objective_terms.into_iter().sum();

    let mut problem = vars.maximise(objective).using(default_solver);

    // Constraint 1: Exactly K bins
    let bin_count: Expression = z
        .iter()
        .flat_map(|row| row.iter().filter_map(|v| *v))
        .sum();
    problem = problem.with(constraint!(bin_count == k as f64));

    // Constraint 2: Each prebin must be in exactly one final bin
    for p in 0..n {
        let mut coverage_terms: Vec<Variable> = Vec::new();
        for i in 0..=p {
            for j in p..n {
                if let Some(var) = z[i][j - i] {
                    coverage_terms.push(var);
                }
            }
        }
        if !coverage_terms.is_empty() {
            let coverage: Expression = coverage_terms.into_iter().sum();
            problem = problem.with(constraint!(coverage == 1.0));
        }
    }

    // Constraint 3: Non-overlapping bins (adjacency)
    // If z[i][j] = 1, then no other bin can use prebins i..=j
    // This is implicitly handled by coverage constraint

    // Constraint 4: Monotonicity (if specified)
    // For each pair of adjacent potential bins, forbid violating pairs
    if monotonicity != MonotonicityConstraint::None
        && monotonicity != MonotonicityConstraint::Peak
        && monotonicity != MonotonicityConstraint::Valley
    {
        for i1 in 0..n {
            for j1 in i1..n {
                let i2 = j1 + 1;
                if i2 >= n {
                    continue;
                }

                for j2 in i2..n {
                    let var1 = z[i1][j1 - i1];
                    let var2 = z[i2][j2 - i2];

                    if let (Some(v1), Some(v2)) = (var1, var2) {
                        let bin1 = get_precomputed_bin(iv_matrix, i1, j1);
                        let bin2 = get_precomputed_bin(iv_matrix, i2, j2);

                        let violates = match monotonicity {
                            MonotonicityConstraint::Ascending => bin1.woe > bin2.woe,
                            MonotonicityConstraint::Descending => bin1.woe < bin2.woe,
                            _ => false,
                        };

                        if violates {
                            // These two bins cannot both be selected
                            let sum: Expression = v1 + v2;
                            problem = problem.with(constraint!(sum <= 1.0));
                        }
                    }
                }
            }
        }
    }

    // Solve the problem
    let solution = problem.solve().context("Failed to solve MIP model")?;

    // Extract solution
    let mut bin_boundaries: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        for j in i..n {
            if let Some(var) = z[i][j - i] {
                let val = solution.value(var);
                if val > 0.5 {
                    bin_boundaries.push((i, j));
                }
            }
        }
    }

    // Sort by start index
    bin_boundaries.sort_by_key(|(start, _)| *start);

    // Calculate total IV
    let total_iv: f64 = bin_boundaries
        .iter()
        .map(|(start, end)| get_precomputed_bin(iv_matrix, *start, *end).iv)
        .sum();

    Ok(SolverResult {
        bin_boundaries,
        total_iv,
        solve_time_ms: start_time.elapsed().as_millis() as u64,
        gap: 0.0, // good_lp doesn't expose gap directly
        monotonicity_applied: monotonicity,
    })
}

/// Reconstruct WoeBin vector from solver solution
pub fn reconstruct_bins(
    prebins: &[WoeBin],
    result: &SolverResult,
    total_events: f64,
    total_non_events: f64,
    total_samples: f64,
) -> Vec<WoeBin> {
    result
        .bin_boundaries
        .iter()
        .map(|(start, end)| {
            // Merge prebins[start..=end] into one bin
            let mut events = 0.0;
            let mut non_events = 0.0;
            let mut count = 0.0;

            for i in *start..=*end {
                events += prebins[i].events;
                non_events += prebins[i].non_events;
                count += prebins[i].count;
            }

            let (woe, iv) = calculate_woe_iv(events, non_events, total_events, total_non_events);
            let event_rate = if count > 0.0 { events / count } else { 0.0 };
            let population_pct = if total_samples > 0.0 {
                count / total_samples * 100.0
            } else {
                0.0
            };

            WoeBin {
                lower_bound: prebins[*start].lower_bound,
                upper_bound: prebins[*end].upper_bound,
                events,
                non_events,
                woe,
                iv_contribution: iv,
                count,
                population_pct,
                event_rate,
            }
        })
        .collect()
}

/// Calculate WoE and IV
fn calculate_woe_iv(
    events: f64,
    non_events: f64,
    total_events: f64,
    total_non_events: f64,
) -> (f64, f64) {
    let dist_events = (events + SMOOTHING) / (total_events + SMOOTHING);
    let dist_non_events = (non_events + SMOOTHING) / (total_non_events + SMOOTHING);
    let woe = (dist_events / dist_non_events).ln();
    let iv = (dist_events - dist_non_events) * woe;
    (woe, iv)
}

/// Solve optimal binning for categorical features
#[allow(dead_code)]
pub fn solve_categorical_binning(
    sorted_categories: &[CategoryStats],
    target_bins: usize,
    config: &SolverConfig,
    total_events: f64,
    total_non_events: f64,
    _total_samples: f64,
) -> Result<SolverResult> {
    let start_time = Instant::now();
    let n = sorted_categories.len();

    // Edge case: if categories <= target_bins, no merging needed
    if n <= target_bins {
        let boundaries: Vec<(usize, usize)> = (0..n).map(|i| (i, i)).collect();
        let total_iv: f64 = sorted_categories
            .iter()
            .map(|c| {
                let (_, iv) = calculate_woe_iv(c.events, c.non_events, total_events, total_non_events);
                iv
            })
            .sum();
        return Ok(SolverResult {
            bin_boundaries: boundaries,
            total_iv,
            solve_time_ms: start_time.elapsed().as_millis() as u64,
            gap: 0.0,
            monotonicity_applied: MonotonicityConstraint::None,
        });
    }

    // Precompute IV for all possible category groupings
    let iv_matrix = precompute_categorical_iv_matrix(sorted_categories, total_events, total_non_events);

    // Solve using the same MIP formulation as numeric binning
    // (categories are already sorted by event rate, so adjacency makes sense)

    let k = target_bins;
    let mut vars = ProblemVariables::new();

    let mut z: Vec<Vec<Option<Variable>>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(n - i);
        for j in i..n {
            let bin = get_precomputed_bin(&iv_matrix, i, j);
            if bin.count >= config.min_bin_samples as f64 {
                row.push(Some(vars.add(variable().binary())));
            } else {
                row.push(None);
            }
        }
        z.push(row);
    }

    // Objective
    let mut objective_terms: Vec<Expression> = Vec::new();
    for i in 0..n {
        for j in i..n {
            if let Some(var) = z[i][j - i] {
                let bin = get_precomputed_bin(&iv_matrix, i, j);
                objective_terms.push(bin.iv * var);
            }
        }
    }

    let objective: Expression = objective_terms.into_iter().sum();
    let mut problem = vars.maximise(objective).using(default_solver);

    // Constraint: Exactly K bins
    let bin_count: Expression = z
        .iter()
        .flat_map(|row| row.iter().filter_map(|v| *v))
        .sum();
    problem = problem.with(constraint!(bin_count == k as f64));

    // Constraint: Coverage
    for p in 0..n {
        let mut coverage_terms: Vec<Variable> = Vec::new();
        for i in 0..=p {
            for j in p..n {
                if let Some(var) = z[i][j - i] {
                    coverage_terms.push(var);
                }
            }
        }
        if !coverage_terms.is_empty() {
            let coverage: Expression = coverage_terms.into_iter().sum();
            problem = problem.with(constraint!(coverage == 1.0));
        }
    }

    // Solve
    let solution = problem.solve().context("Failed to solve categorical MIP model")?;

    // Extract solution
    let mut bin_boundaries: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        for j in i..n {
            if let Some(var) = z[i][j - i] {
                let val = solution.value(var);
                if val > 0.5 {
                    bin_boundaries.push((i, j));
                }
            }
        }
    }

    bin_boundaries.sort_by_key(|(start, _)| *start);

    let total_iv: f64 = bin_boundaries
        .iter()
        .map(|(start, end)| get_precomputed_bin(&iv_matrix, *start, *end).iv)
        .sum();

    Ok(SolverResult {
        bin_boundaries,
        total_iv,
        solve_time_ms: start_time.elapsed().as_millis() as u64,
        gap: 0.0,
        monotonicity_applied: config.monotonicity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_prebins() -> Vec<WoeBin> {
        vec![
            WoeBin {
                lower_bound: 0.0,
                upper_bound: 10.0,
                events: 5.0,
                non_events: 15.0,
                woe: -0.693,
                iv_contribution: 0.069,
                count: 20.0,
                population_pct: 33.3,
                event_rate: 0.25,
            },
            WoeBin {
                lower_bound: 10.0,
                upper_bound: 20.0,
                events: 10.0,
                non_events: 10.0,
                woe: 0.0,
                iv_contribution: 0.0,
                count: 20.0,
                population_pct: 33.3,
                event_rate: 0.5,
            },
            WoeBin {
                lower_bound: 20.0,
                upper_bound: 30.0,
                events: 15.0,
                non_events: 5.0,
                woe: 0.693,
                iv_contribution: 0.069,
                count: 20.0,
                population_pct: 33.3,
                event_rate: 0.75,
            },
        ]
    }

    #[test]
    fn test_solve_no_merging_needed() {
        let prebins = create_test_prebins();
        let config = SolverConfig::default();

        // Request 3 bins from 3 prebins - no merging needed
        let result = solve_numeric_binning(&prebins, 3, &config, 30.0, 30.0, 60.0);
        assert!(result.is_ok());

        let res = result.unwrap();
        assert_eq!(res.bin_boundaries.len(), 3);
        assert_eq!(res.bin_boundaries[0], (0, 0));
        assert_eq!(res.bin_boundaries[1], (1, 1));
        assert_eq!(res.bin_boundaries[2], (2, 2));
    }

    #[test]
    fn test_solve_merge_to_two_bins() {
        let prebins = create_test_prebins();
        let config = SolverConfig::default();

        // Request 2 bins from 3 prebins
        let result = solve_numeric_binning(&prebins, 2, &config, 30.0, 30.0, 60.0);
        assert!(result.is_ok());

        let res = result.unwrap();
        assert_eq!(res.bin_boundaries.len(), 2);
        assert!(res.total_iv > 0.0);
    }

    #[test]
    fn test_solve_merge_to_one_bin() {
        let prebins = create_test_prebins();
        let config = SolverConfig::default();

        // Request 1 bin from 3 prebins
        let result = solve_numeric_binning(&prebins, 1, &config, 30.0, 30.0, 60.0);
        assert!(result.is_ok());

        let res = result.unwrap();
        assert_eq!(res.bin_boundaries.len(), 1);
        assert_eq!(res.bin_boundaries[0], (0, 2));
    }

    #[test]
    fn test_reconstruct_bins() {
        let prebins = create_test_prebins();
        let result = SolverResult {
            bin_boundaries: vec![(0, 1), (2, 2)],
            total_iv: 0.1,
            solve_time_ms: 10,
            gap: 0.0,
            monotonicity_applied: MonotonicityConstraint::None,
        };

        let bins = reconstruct_bins(&prebins, &result, 30.0, 30.0, 60.0);

        assert_eq!(bins.len(), 2);

        // First bin merges prebins 0 and 1
        assert_eq!(bins[0].lower_bound, 0.0);
        assert_eq!(bins[0].upper_bound, 20.0);
        assert_eq!(bins[0].events, 15.0);
        assert_eq!(bins[0].non_events, 25.0);

        // Second bin is just prebin 2
        assert_eq!(bins[1].lower_bound, 20.0);
        assert_eq!(bins[1].upper_bound, 30.0);
        assert_eq!(bins[1].events, 15.0);
        assert_eq!(bins[1].non_events, 5.0);
    }
}
