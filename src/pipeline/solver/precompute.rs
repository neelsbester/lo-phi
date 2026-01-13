//! Precomputation of IV values for all possible bin combinations
//!
//! This module efficiently computes IV for all possible merged bin configurations,
//! which is needed by the MIP solver to evaluate objective function coefficients.

use super::super::iv::WoeBin;
use super::CategoryStats;

/// Smoothing constant to avoid log(0) in WoE calculation
const SMOOTHING: f64 = 0.5;

/// Precomputed IV and WoE values for a potential merged bin
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PrecomputedBin {
    /// Start prebin index (inclusive)
    pub start: usize,
    /// End prebin index (inclusive)
    pub end: usize,
    /// Total events in merged bin
    pub events: f64,
    /// Total non-events in merged bin
    pub non_events: f64,
    /// Total count in merged bin
    pub count: f64,
    /// WoE of merged bin
    pub woe: f64,
    /// IV contribution of merged bin
    pub iv: f64,
}

/// Calculate WoE and IV for given event/non-event counts
fn calculate_woe_iv(
    events: f64,
    non_events: f64,
    total_events: f64,
    total_non_events: f64,
) -> (f64, f64) {
    // Apply Laplace smoothing to avoid log(0)
    let dist_events = (events + SMOOTHING) / (total_events + SMOOTHING);
    let dist_non_events = (non_events + SMOOTHING) / (total_non_events + SMOOTHING);

    // WoE = ln(%events / %non_events)
    // Using %bad/%good convention: positive WoE = higher risk
    let woe = (dist_events / dist_non_events).ln();

    // IV contribution = (% events - % non_events) * WoE
    let iv = (dist_events - dist_non_events) * woe;

    (woe, iv)
}

/// Precompute IV for all possible merged bin combinations
///
/// Returns a 2D vector where iv_matrix[i][j] contains the PrecomputedBin
/// for merging prebins i through j (inclusive).
///
/// Uses cumulative sums for O(n^2) complexity instead of O(n^3).
#[allow(clippy::needless_range_loop)]
pub fn precompute_iv_matrix(
    prebins: &[WoeBin],
    total_events: f64,
    total_non_events: f64,
) -> Vec<Vec<PrecomputedBin>> {
    let n = prebins.len();
    let mut matrix = Vec::with_capacity(n);

    for i in 0..n {
        let mut row = Vec::with_capacity(n - i);
        let mut cumulative_events = 0.0;
        let mut cumulative_non_events = 0.0;
        let mut cumulative_count = 0.0;

        for j in i..n {
            cumulative_events += prebins[j].events;
            cumulative_non_events += prebins[j].non_events;
            cumulative_count += prebins[j].count;

            let (woe, iv) = calculate_woe_iv(
                cumulative_events,
                cumulative_non_events,
                total_events,
                total_non_events,
            );

            row.push(PrecomputedBin {
                start: i,
                end: j,
                events: cumulative_events,
                non_events: cumulative_non_events,
                count: cumulative_count,
                woe,
                iv,
            });
        }
        matrix.push(row);
    }

    matrix
}

/// Get a precomputed bin from the matrix
///
/// Returns the PrecomputedBin for merging prebins[start] through prebins[end].
#[inline]
pub fn get_precomputed_bin(
    matrix: &[Vec<PrecomputedBin>],
    start: usize,
    end: usize,
) -> &PrecomputedBin {
    &matrix[start][end - start]
}

/// Precompute IV for all possible category groupings
///
/// Similar to numeric binning but for categories sorted by event rate.
#[allow(clippy::needless_range_loop)]
pub fn precompute_categorical_iv_matrix(
    categories: &[CategoryStats],
    total_events: f64,
    total_non_events: f64,
) -> Vec<Vec<PrecomputedBin>> {
    let n = categories.len();
    let mut matrix = Vec::with_capacity(n);

    for i in 0..n {
        let mut row = Vec::with_capacity(n - i);
        let mut cumulative_events = 0.0;
        let mut cumulative_non_events = 0.0;
        let mut cumulative_count = 0.0;

        for j in i..n {
            cumulative_events += categories[j].events;
            cumulative_non_events += categories[j].non_events;
            cumulative_count += categories[j].count;

            let (woe, iv) = calculate_woe_iv(
                cumulative_events,
                cumulative_non_events,
                total_events,
                total_non_events,
            );

            row.push(PrecomputedBin {
                start: i,
                end: j,
                events: cumulative_events,
                non_events: cumulative_non_events,
                count: cumulative_count,
                woe,
                iv,
            });
        }
        matrix.push(row);
    }

    matrix
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
                woe: 0.0,
                iv_contribution: 0.0,
                count: 20.0,
                population_pct: 20.0,
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
                population_pct: 20.0,
                event_rate: 0.5,
            },
            WoeBin {
                lower_bound: 20.0,
                upper_bound: 30.0,
                events: 15.0,
                non_events: 5.0,
                woe: 0.0,
                iv_contribution: 0.0,
                count: 20.0,
                population_pct: 20.0,
                event_rate: 0.75,
            },
        ]
    }

    #[test]
    fn test_precompute_iv_matrix_dimensions() {
        let prebins = create_test_prebins();
        let matrix = precompute_iv_matrix(&prebins, 30.0, 30.0);

        // Should have 3 rows
        assert_eq!(matrix.len(), 3);

        // Row 0 should have 3 elements (bins 0-0, 0-1, 0-2)
        assert_eq!(matrix[0].len(), 3);

        // Row 1 should have 2 elements (bins 1-1, 1-2)
        assert_eq!(matrix[1].len(), 2);

        // Row 2 should have 1 element (bin 2-2)
        assert_eq!(matrix[2].len(), 1);
    }

    #[test]
    fn test_precompute_iv_matrix_single_bins() {
        let prebins = create_test_prebins();
        let matrix = precompute_iv_matrix(&prebins, 30.0, 30.0);

        // Single bin [0,0] should have events=5, non_events=15
        let bin_0_0 = get_precomputed_bin(&matrix, 0, 0);
        assert_eq!(bin_0_0.events, 5.0);
        assert_eq!(bin_0_0.non_events, 15.0);
        assert_eq!(bin_0_0.count, 20.0);

        // Single bin [1,1] should have events=10, non_events=10
        let bin_1_1 = get_precomputed_bin(&matrix, 1, 1);
        assert_eq!(bin_1_1.events, 10.0);
        assert_eq!(bin_1_1.non_events, 10.0);
    }

    #[test]
    fn test_precompute_iv_matrix_merged_bins() {
        let prebins = create_test_prebins();
        let matrix = precompute_iv_matrix(&prebins, 30.0, 30.0);

        // Merged bin [0,1] should have events=15, non_events=25
        let bin_0_1 = get_precomputed_bin(&matrix, 0, 1);
        assert_eq!(bin_0_1.events, 15.0);
        assert_eq!(bin_0_1.non_events, 25.0);
        assert_eq!(bin_0_1.count, 40.0);

        // Merged bin [0,2] should have all events/non-events
        let bin_0_2 = get_precomputed_bin(&matrix, 0, 2);
        assert_eq!(bin_0_2.events, 30.0);
        assert_eq!(bin_0_2.non_events, 30.0);
        assert_eq!(bin_0_2.count, 60.0);
    }

    #[test]
    fn test_iv_is_non_negative() {
        let prebins = create_test_prebins();
        let matrix = precompute_iv_matrix(&prebins, 30.0, 30.0);

        // All IV values should be non-negative
        for row in &matrix {
            for bin in row {
                assert!(bin.iv >= 0.0, "IV should be non-negative, got {}", bin.iv);
            }
        }
    }
}
