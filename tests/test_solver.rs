//! Tests for solver-based optimal binning

use polars::prelude::*;

use lophi::pipeline::{analyze_features_iv, BinningStrategy, MonotonicityConstraint, SolverConfig};

/// Create test dataframe with numeric feature that has clear event rate separation
fn create_numeric_test_dataframe() -> DataFrame {
    // Feature values: lower values have lower event rate, higher values have higher event rate
    df! {
        "target" => [0i32, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1,
                     0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1],
        "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 50.0, 51.0, 52.0, 53.0, 54.0, 55.0, 56.0, 57.0,
                      1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 50.5, 51.5, 52.5, 53.5, 54.5, 55.5, 56.5, 57.5],
    }
    .unwrap()
}

#[test]
fn test_solver_produces_expected_bin_count() {
    let df = create_numeric_test_dataframe();
    let weights = vec![1.0; df.height()];

    let solver_config = SolverConfig {
        timeout_seconds: 30,
        gap_tolerance: 0.01,
        monotonicity: MonotonicityConstraint::None,
        min_bin_samples: 5,
    };

    let result = analyze_features_iv(
        &df,
        "target",
        3, // Target 3 bins
        20,
        None,
        BinningStrategy::Cart,
        None,
        None,
        &weights,
        None,
        Some(&solver_config),
    );

    assert!(result.is_ok(), "Solver analysis should succeed");

    let analyses = result.unwrap();
    assert_eq!(analyses.len(), 1, "Should have one feature analysis");

    let analysis = &analyses[0];
    assert!(
        !analysis.bins.is_empty(),
        "Should have bins after solver optimization"
    );
    // Should have at most 3 bins (might have fewer if data doesn't support)
    assert!(
        analysis.bins.len() <= 3,
        "Should have at most 3 bins, got {}",
        analysis.bins.len()
    );
}

#[test]
fn test_solver_with_ascending_monotonicity() {
    let df = create_numeric_test_dataframe();
    let weights = vec![1.0; df.height()];

    let solver_config = SolverConfig {
        timeout_seconds: 30,
        gap_tolerance: 0.01,
        monotonicity: MonotonicityConstraint::Ascending,
        min_bin_samples: 5,
    };

    let result = analyze_features_iv(
        &df,
        "target",
        5,
        20,
        None,
        BinningStrategy::Cart,
        None,
        None,
        &weights,
        None,
        Some(&solver_config),
    );

    assert!(
        result.is_ok(),
        "Solver with ascending monotonicity should succeed"
    );

    let analyses = result.unwrap();
    let analysis = &analyses[0];

    // For ascending monotonicity, WoE should increase across bins
    if analysis.bins.len() >= 2 {
        for i in 1..analysis.bins.len() {
            // Allow small tolerance for numerical precision
            let prev_woe = analysis.bins[i - 1].woe;
            let curr_woe = analysis.bins[i].woe;
            assert!(
                curr_woe >= prev_woe - 0.01,
                "WoE should be ascending: bin {} has WoE {}, bin {} has WoE {}",
                i - 1,
                prev_woe,
                i,
                curr_woe
            );
        }
    }
}

#[test]
fn test_solver_with_descending_monotonicity() {
    // Create data where lower feature values have higher event rate
    let df = df! {
        "target" => [1i32, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0,
                     1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
        "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 50.0, 51.0, 52.0, 53.0, 54.0, 55.0, 56.0, 57.0,
                      1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 50.5, 51.5, 52.5, 53.5, 54.5, 55.5, 56.5, 57.5],
    }
    .unwrap();

    let weights = vec![1.0; df.height()];

    let solver_config = SolverConfig {
        timeout_seconds: 30,
        gap_tolerance: 0.01,
        monotonicity: MonotonicityConstraint::Descending,
        min_bin_samples: 5,
    };

    let result = analyze_features_iv(
        &df,
        "target",
        5,
        20,
        None,
        BinningStrategy::Cart,
        None,
        None,
        &weights,
        None,
        Some(&solver_config),
    );

    assert!(
        result.is_ok(),
        "Solver with descending monotonicity should succeed"
    );

    let analyses = result.unwrap();
    let analysis = &analyses[0];

    // For descending monotonicity, WoE should decrease across bins
    if analysis.bins.len() >= 2 {
        for i in 1..analysis.bins.len() {
            let prev_woe = analysis.bins[i - 1].woe;
            let curr_woe = analysis.bins[i].woe;
            assert!(
                curr_woe <= prev_woe + 0.01,
                "WoE should be descending: bin {} has WoE {}, bin {} has WoE {}",
                i - 1,
                prev_woe,
                i,
                curr_woe
            );
        }
    }
}

#[test]
fn test_solver_auto_monotonicity() {
    let df = create_numeric_test_dataframe();
    let weights = vec![1.0; df.height()];

    let solver_config = SolverConfig {
        timeout_seconds: 30,
        gap_tolerance: 0.01,
        monotonicity: MonotonicityConstraint::Auto,
        min_bin_samples: 5,
    };

    let result = analyze_features_iv(
        &df,
        "target",
        5,
        20,
        None,
        BinningStrategy::Cart,
        None,
        None,
        &weights,
        None,
        Some(&solver_config),
    );

    assert!(
        result.is_ok(),
        "Solver with auto monotonicity should succeed"
    );

    let analyses = result.unwrap();
    let analysis = &analyses[0];

    // Should have produced some bins
    assert!(!analysis.bins.is_empty(), "Should have bins");

    // IV should be non-negative
    assert!(analysis.iv >= 0.0, "IV should be non-negative");
}

#[test]
fn test_greedy_vs_solver_produces_valid_output() {
    let df = create_numeric_test_dataframe();
    let weights = vec![1.0; df.height()];

    // First run without solver (greedy)
    let greedy_result = analyze_features_iv(
        &df,
        "target",
        5,
        20,
        None,
        BinningStrategy::Cart,
        None,
        None,
        &weights,
        None,
        None,
    );

    assert!(greedy_result.is_ok(), "Greedy analysis should succeed");
    let greedy_analyses = greedy_result.unwrap();
    let greedy_iv = greedy_analyses[0].iv;

    // Then run with solver
    let solver_config = SolverConfig {
        timeout_seconds: 30,
        gap_tolerance: 0.01,
        monotonicity: MonotonicityConstraint::None,
        min_bin_samples: 5,
    };

    let solver_result = analyze_features_iv(
        &df,
        "target",
        5,
        20,
        None,
        BinningStrategy::Cart,
        None,
        None,
        &weights,
        None,
        Some(&solver_config),
    );

    assert!(solver_result.is_ok(), "Solver analysis should succeed");
    let solver_analyses = solver_result.unwrap();
    let solver_iv = solver_analyses[0].iv;

    // Both should produce non-negative IV
    assert!(
        greedy_iv >= 0.0,
        "Greedy IV should be non-negative: {}",
        greedy_iv
    );
    assert!(
        solver_iv >= 0.0,
        "Solver IV should be non-negative: {}",
        solver_iv
    );

    // Solver should produce IV that is at least as good (with some tolerance for numerical precision)
    // The MIP solver maximizes IV, so it should be >= greedy solution
    assert!(
        solver_iv >= greedy_iv - 0.001,
        "Solver IV ({}) should be >= greedy IV ({})",
        solver_iv,
        greedy_iv
    );
}

#[test]
fn test_solver_bins_cover_all_data() {
    let df = create_numeric_test_dataframe();
    let weights = vec![1.0; df.height()];
    let total_count = df.height() as f64;

    let solver_config = SolverConfig {
        timeout_seconds: 30,
        gap_tolerance: 0.01,
        monotonicity: MonotonicityConstraint::None,
        min_bin_samples: 5,
    };

    let result = analyze_features_iv(
        &df,
        "target",
        3,
        20,
        None,
        BinningStrategy::Cart,
        None,
        None,
        &weights,
        None,
        Some(&solver_config),
    );

    assert!(result.is_ok(), "Solver analysis should succeed");

    let analyses = result.unwrap();
    let analysis = &analyses[0];

    // Sum of all bin counts should equal total samples
    let bin_count_sum: f64 = analysis.bins.iter().map(|b| b.count).sum();
    assert!(
        (bin_count_sum - total_count).abs() < 0.01,
        "Bins should cover all data: bin sum = {}, total = {}",
        bin_count_sum,
        total_count
    );
}
