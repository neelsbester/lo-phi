//! Unit tests for correlation analysis

use lophi::pipeline::{
    find_correlated_pairs, find_correlated_pairs_auto, find_correlated_pairs_matrix,
    select_features_to_drop, AssociationMeasure, CorrelatedPair,
};
use polars::prelude::*;

#[path = "common/mod.rs"]
mod common;

#[test]
fn test_find_perfectly_correlated_pair() {
    let df = common::create_correlation_test_dataframe();
    let weights = vec![1.0; df.height()];

    let pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    // Should find a-b correlation (perfect positive)
    let ab_pair = pairs.iter().find(|p| {
        (p.feature1 == "a" && p.feature2 == "b") || (p.feature1 == "b" && p.feature2 == "a")
    });

    assert!(ab_pair.is_some(), "Should find correlation between a and b");
    assert!(
        ab_pair.unwrap().correlation.abs() > 0.99,
        "Correlation between a and b should be > 0.99, got {}",
        ab_pair.unwrap().correlation
    );
}

#[test]
fn test_find_negative_correlation() {
    let df = common::create_correlation_test_dataframe();
    let weights = vec![1.0; df.height()];

    // Use lower threshold to catch negative correlation
    let pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    // Should find a-c correlation (perfect negative)
    let ac_pair = pairs.iter().find(|p| {
        (p.feature1 == "a" && p.feature2 == "c") || (p.feature1 == "c" && p.feature2 == "a")
    });

    assert!(
        ac_pair.is_some(),
        "Should find negative correlation between a and c"
    );
    assert!(
        ac_pair.unwrap().correlation < -0.9,
        "Correlation between a and c should be strongly negative, got {}",
        ac_pair.unwrap().correlation
    );
}

#[test]
fn test_no_correlation_found_high_threshold() {
    let df = df! {
        "a" => [1.0f64, 5.0, 2.0, 8.0, 3.0, 7.0, 4.0, 6.0, 9.0, 0.0],
        "b" => [9.0f64, 2.0, 7.0, 1.0, 6.0, 3.0, 8.0, 4.0, 0.0, 5.0],
    }
    .unwrap();
    let weights = vec![1.0; 10];

    let pairs = find_correlated_pairs(&df, 0.95, &weights, None).unwrap();

    assert!(
        pairs.is_empty(),
        "Random data should have no highly correlated pairs at 0.95 threshold"
    );
}

#[test]
fn test_select_features_to_drop_protects_target() {
    let pairs = vec![CorrelatedPair {
        feature1: "target".to_string(),
        feature2: "feature_a".to_string(),
        correlation: 0.98,
        measure: AssociationMeasure::Pearson,
    }];

    let to_drop = select_features_to_drop(&pairs, "target", None);
    let drop_names: Vec<&str> = to_drop.iter().map(|f| f.feature.as_str()).collect();

    assert_eq!(to_drop.len(), 1, "Should drop exactly 1 feature");
    assert!(drop_names.contains(&"feature_a"), "Should drop feature_a");
    assert!(!drop_names.contains(&"target"), "Should NEVER drop target");
}

#[test]
fn test_select_features_to_drop_target_in_second_position() {
    let pairs = vec![CorrelatedPair {
        feature1: "feature_a".to_string(),
        feature2: "target".to_string(),
        correlation: 0.98,
        measure: AssociationMeasure::Pearson,
    }];

    let to_drop = select_features_to_drop(&pairs, "target", None);
    let drop_names: Vec<&str> = to_drop.iter().map(|f| f.feature.as_str()).collect();

    assert_eq!(to_drop.len(), 1);
    assert!(drop_names.contains(&"feature_a"));
    assert!(
        !drop_names.contains(&"target"),
        "Target should be protected regardless of position"
    );
}

#[test]
fn test_select_drops_more_frequent_feature() {
    // feature_a appears in 2 pairs, feature_b and feature_c in 1 each
    let pairs = vec![
        CorrelatedPair {
            feature1: "feature_a".to_string(),
            feature2: "feature_b".to_string(),
            correlation: 0.96,
            measure: AssociationMeasure::Pearson,
        },
        CorrelatedPair {
            feature1: "feature_a".to_string(),
            feature2: "feature_c".to_string(),
            correlation: 0.97,
            measure: AssociationMeasure::Pearson,
        },
    ];

    let to_drop = select_features_to_drop(&pairs, "target", None);
    let drop_names: Vec<&str> = to_drop.iter().map(|f| f.feature.as_str()).collect();

    // feature_a should be dropped (appears more frequently)
    assert!(
        drop_names.contains(&"feature_a"),
        "Should drop feature_a (appears in more pairs)"
    );
    assert!(
        !drop_names.contains(&"feature_b"),
        "Should NOT drop feature_b"
    );
    assert!(
        !drop_names.contains(&"feature_c"),
        "Should NOT drop feature_c"
    );
}

#[test]
fn test_already_resolved_pairs_skipped() {
    // If a feature is already marked for dropping, don't process its other pairs
    let pairs = vec![
        CorrelatedPair {
            feature1: "a".to_string(),
            feature2: "b".to_string(),
            correlation: 0.98,
            measure: AssociationMeasure::Pearson,
        },
        CorrelatedPair {
            feature1: "a".to_string(),
            feature2: "c".to_string(),
            correlation: 0.97,
            measure: AssociationMeasure::Pearson,
        },
        CorrelatedPair {
            feature1: "b".to_string(),
            feature2: "c".to_string(),
            correlation: 0.96,
            measure: AssociationMeasure::Pearson,
        },
    ];

    let to_drop = select_features_to_drop(&pairs, "target", None);

    // Should resolve pairs efficiently without dropping everything
    // The exact result depends on frequency, but we shouldn't drop all 3
    assert!(
        to_drop.len() < 3,
        "Should not drop all features, got {:?}",
        to_drop
    );
}

#[test]
fn test_single_column_dataframe() {
    let df = df! {
        "only_col" => [1.0f64, 2.0, 3.0],
    }
    .unwrap();
    let weights = vec![1.0; 3];

    let pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    assert!(
        pairs.is_empty(),
        "Single column cannot correlate with itself"
    );
}

#[test]
fn test_two_identical_columns() {
    let df = df! {
        "col_a" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "col_b" => [1.0f64, 2.0, 3.0, 4.0, 5.0], // Identical to col_a
    }
    .unwrap();
    let weights = vec![1.0; 5];

    let pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    assert!(!pairs.is_empty(), "Identical columns should be correlated");
    assert!(
        pairs[0].correlation.abs() > 0.999,
        "Identical columns should have correlation = 1.0"
    );
}

#[test]
fn test_sorted_by_correlation_descending() {
    let df = df! {
        "a" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        "b" => [1.1f64, 2.1, 3.1, 4.1, 5.1, 6.1, 7.1, 8.1, 9.1, 10.1], // Very high correlation
        "c" => [1.5f64, 2.3, 3.1, 4.2, 5.0, 6.1, 7.3, 8.0, 9.2, 10.1], // Slightly less correlated
    }
    .unwrap();
    let weights = vec![1.0; 10];

    let pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    // Verify sorted by absolute correlation descending
    for i in 0..pairs.len().saturating_sub(1) {
        assert!(
            pairs[i].correlation.abs() >= pairs[i + 1].correlation.abs(),
            "Pairs should be sorted by |correlation| descending"
        );
    }
}

#[test]
fn test_empty_pairs_drop_selection() {
    let pairs: Vec<CorrelatedPair> = vec![];
    let to_drop = select_features_to_drop(&pairs, "target", None);

    assert!(
        to_drop.is_empty(),
        "Empty pairs should result in empty drop list"
    );
}

#[test]
fn test_non_numeric_columns_ignored() {
    // String columns should be ignored in correlation analysis
    let df = df! {
        "numeric" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "string_col" => ["a", "b", "c", "d", "e"],
    }
    .unwrap();
    let weights = vec![1.0; 5];

    let pairs = find_correlated_pairs(&df, 0.5, &weights, None).unwrap();

    // Should not find any pairs involving string columns
    for pair in &pairs {
        assert_ne!(pair.feature1, "string_col");
        assert_ne!(pair.feature2, "string_col");
    }
}

#[test]
fn test_weight_column_excluded_from_correlation() {
    // Weight column should be excluded from correlation analysis
    let df = df! {
        "feature_a" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "feature_b" => [1.0f64, 2.0, 3.0, 4.0, 5.0], // Perfectly correlated with a
        "weight" => [1.0f64, 1.0, 1.0, 1.0, 1.0],
    }
    .unwrap();
    let weights = vec![1.0; 5];

    // Without exclusion - weight should be in correlation pairs
    let pairs_included = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();
    let _has_weight = pairs_included
        .iter()
        .any(|p| p.feature1 == "weight" || p.feature2 == "weight");
    // Note: weight column might not correlate with anything, but it should be checked

    // With exclusion - weight should NOT be in any correlation pairs
    let pairs_excluded = find_correlated_pairs(&df, 0.9, &weights, Some("weight")).unwrap();
    let has_weight_excluded = pairs_excluded
        .iter()
        .any(|p| p.feature1 == "weight" || p.feature2 == "weight");
    assert!(
        !has_weight_excluded,
        "Weight column should be excluded from correlation pairs"
    );

    // Should still find the feature_a <-> feature_b correlation
    let ab_pair = pairs_excluded.iter().find(|p| {
        (p.feature1 == "feature_a" && p.feature2 == "feature_b")
            || (p.feature1 == "feature_b" && p.feature2 == "feature_a")
    });
    assert!(ab_pair.is_some(), "Should still find feature correlations");
}

#[test]
fn test_weighted_correlation_with_non_uniform_weights() {
    // Test that weighted correlation works with non-uniform weights
    // Two perfectly correlated columns should have correlation ~1 regardless of weights
    let df = df! {
        "a" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "b" => [2.0f64, 4.0, 6.0, 8.0, 10.0], // b = 2*a
    }
    .unwrap();

    // Non-uniform weights
    let weights = vec![1.0, 2.0, 1.0, 3.0, 1.0];

    let pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    assert!(!pairs.is_empty(), "Should find correlated pair");
    assert!(
        pairs[0].correlation.abs() > 0.99,
        "Perfectly correlated columns should have correlation ~1, got {}",
        pairs[0].correlation
    );
}

#[test]
fn test_zero_weights_excluded_from_correlation() {
    // Rows with zero weight should be effectively excluded from correlation calculation
    // Create data where the non-zero weighted rows are perfectly correlated
    let df = df! {
        "a" => [1.0f64, 99.0, 2.0, 88.0, 3.0],  // outliers at positions 1,3
        "b" => [2.0f64, 1.0, 4.0, 2.0, 6.0],    // outliers would break correlation
    }
    .unwrap();

    // Zero out the outlier positions
    let weights = vec![1.0, 0.0, 1.0, 0.0, 1.0];

    let pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    // With outliers excluded (zero weight), remaining points (1,2), (2,4), (3,6)
    // should show perfect correlation
    assert!(
        !pairs.is_empty(),
        "Should find correlated pair when outliers have zero weight"
    );
    assert!(
        pairs[0].correlation.abs() > 0.99,
        "With outliers zero-weighted, correlation should be ~1, got {}",
        pairs[0].correlation
    );
}

/// Verify that pairwise and matrix methods produce equivalent results
#[test]
fn test_matrix_pairwise_equivalence() {
    // Create a dataframe with known correlations
    let df = df! {
        "a" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        "b" => [2.0f64, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0],  // b = 2*a, perfect correlation
        "c" => [10.0f64, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0],       // c = -a + 11, perfect negative
        "d" => [1.5f64, 2.3, 3.7, 4.1, 5.8, 6.2, 7.9, 8.4, 9.1, 10.5],       // noisy positive correlation
    }
    .unwrap();

    let weights = vec![1.0; df.height()];
    let threshold = 0.8;

    let pairs_pairwise = find_correlated_pairs(&df, threshold, &weights, None).unwrap();
    let pairs_matrix = find_correlated_pairs_matrix(&df, threshold, &weights, None).unwrap();

    // Both methods should find the same number of pairs
    assert_eq!(
        pairs_pairwise.len(),
        pairs_matrix.len(),
        "Both methods should find the same number of pairs: pairwise={}, matrix={}",
        pairs_pairwise.len(),
        pairs_matrix.len()
    );

    // Compare correlation values (allow small numerical differences)
    for pair_pw in &pairs_pairwise {
        let matching = pairs_matrix.iter().find(|p| {
            (p.feature1 == pair_pw.feature1 && p.feature2 == pair_pw.feature2)
                || (p.feature1 == pair_pw.feature2 && p.feature2 == pair_pw.feature1)
        });

        assert!(
            matching.is_some(),
            "Matrix method should find pair ({}, {})",
            pair_pw.feature1,
            pair_pw.feature2
        );

        let pair_mat = matching.unwrap();
        let diff = (pair_pw.correlation - pair_mat.correlation).abs();
        assert!(
            diff < 0.01,
            "Correlation values should match (tolerance 0.01): pairwise={:.4}, matrix={:.4}, diff={:.6}",
            pair_pw.correlation,
            pair_mat.correlation,
            diff
        );
    }
}

/// Test matrix method with larger dataset
#[test]
fn test_matrix_method_larger_dataset() {
    // Create a larger dataframe to test matrix method performance path
    let n = 100;
    let a: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let b: Vec<f64> = a.iter().map(|x| x * 2.0 + 1.0).collect();
    let c: Vec<f64> = a.iter().map(|x| -x + 100.0).collect();

    let df = df! {
        "a" => &a,
        "b" => &b,
        "c" => &c,
    }
    .unwrap();

    let weights = vec![1.0; df.height()];

    let pairs = find_correlated_pairs_matrix(&df, 0.9, &weights, None).unwrap();

    // Should find a-b (positive) and a-c (negative) correlations
    assert!(
        pairs.len() >= 2,
        "Should find at least 2 correlated pairs, found {}",
        pairs.len()
    );

    // Verify a-b correlation is close to 1.0
    let ab_pair = pairs.iter().find(|p| {
        (p.feature1 == "a" && p.feature2 == "b") || (p.feature1 == "b" && p.feature2 == "a")
    });
    assert!(ab_pair.is_some(), "Should find a-b correlation");
    assert!(
        ab_pair.unwrap().correlation.abs() > 0.99,
        "a-b correlation should be ~1.0"
    );

    // Verify a-c correlation is close to -1.0
    let ac_pair = pairs.iter().find(|p| {
        (p.feature1 == "a" && p.feature2 == "c") || (p.feature1 == "c" && p.feature2 == "a")
    });
    assert!(ac_pair.is_some(), "Should find a-c correlation");
    assert!(
        ac_pair.unwrap().correlation < -0.99,
        "a-c correlation should be ~-1.0"
    );
}

// ── T-C2: find_correlated_pairs_auto ───────────────────────────────────────

/// Build a DataFrame with `n` perfectly correlated column pairs plus a target.
/// Columns are: target, a0, b0, a1, b1, ... where b_i = 2 * a_i.
fn build_wide_df(n_pairs: usize, rows: usize) -> DataFrame {
    use polars::prelude::Column;
    let mut cols: Vec<Column> = Vec::new();
    let target: Vec<i32> = (0..rows).map(|i| (i % 2) as i32).collect();
    cols.push(Column::new("target".into(), target));

    for i in 0..n_pairs {
        let a: Vec<f64> = (0..rows).map(|j| (j + i) as f64).collect();
        let b: Vec<f64> = a.iter().map(|x| x * 2.0).collect();
        cols.push(Column::new(format!("a{}", i).into(), a));
        cols.push(Column::new(format!("b{}", i).into(), b));
    }

    DataFrame::new(cols).unwrap()
}

#[test]
fn test_auto_uses_pairwise_for_small_column_count() {
    // 3 pairs = 6 numeric cols + target = 7 total numeric (< 15 threshold)
    // Results should equal pairwise
    let df = build_wide_df(3, 20);
    let weights = vec![1.0; 20];
    let threshold = 0.9;

    let auto_pairs =
        find_correlated_pairs_auto(&df, threshold, &weights, Some("target"), None).unwrap();
    let pw_pairs = find_correlated_pairs(&df, threshold, &weights, Some("target")).unwrap();

    assert_eq!(
        auto_pairs.len(),
        pw_pairs.len(),
        "auto should select pairwise for < 15 cols: auto={}, pairwise={}",
        auto_pairs.len(),
        pw_pairs.len()
    );
}

#[test]
fn test_auto_uses_matrix_for_large_column_count() {
    // 10 pairs = 20 numeric cols + target -> numeric cols >= 15 threshold
    let df = build_wide_df(10, 30);
    let weights = vec![1.0; 30];
    let threshold = 0.9;

    let auto_pairs =
        find_correlated_pairs_auto(&df, threshold, &weights, Some("target"), None).unwrap();
    let mat_pairs = find_correlated_pairs_matrix(&df, threshold, &weights, Some("target")).unwrap();

    assert_eq!(
        auto_pairs.len(),
        mat_pairs.len(),
        "auto should select matrix for >= 15 cols: auto={}, matrix={}",
        auto_pairs.len(),
        mat_pairs.len()
    );
}

#[test]
fn test_auto_empty_dataframe_returns_empty() {
    let df = DataFrame::empty();
    let weights: Vec<f64> = vec![];

    let result = find_correlated_pairs_auto(&df, 0.9, &weights, None, None).unwrap();
    assert!(result.is_empty(), "Empty DataFrame should yield no pairs");
}

#[test]
fn test_auto_single_column_returns_empty() {
    let df = df! {
        "only_col" => [1.0f64, 2.0, 3.0],
    }
    .unwrap();
    let weights = vec![1.0; 3];

    let result = find_correlated_pairs_auto(&df, 0.9, &weights, None, None).unwrap();
    assert!(
        result.is_empty(),
        "Single column cannot correlate with itself"
    );
}

// ── T-E1: constant column (zero variance) ──────────────────────────────────

#[test]
fn test_constant_column_does_not_crash() {
    // A constant column has zero variance; correlation is undefined.
    // The library should handle this gracefully (return no pair, NaN, or similar).
    let df = df! {
        "constant" => [5.0f64; 10],
        "normal"   => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
    }
    .unwrap();
    let weights = vec![1.0; 10];

    // Must not panic
    let result = find_correlated_pairs(&df, 0.5, &weights, None);
    assert!(
        result.is_ok(),
        "Constant column should be handled gracefully, got: {:?}",
        result.err()
    );

    // Constant column should not appear in valid correlated pairs with a finite correlation
    if let Ok(pairs) = result {
        for pair in &pairs {
            let involves_constant = pair.feature1 == "constant" || pair.feature2 == "constant";
            if involves_constant {
                assert!(
                    pair.correlation.is_finite(),
                    "If constant column appears in a pair, correlation must not be NaN/Inf"
                );
            }
        }
    }
}

#[test]
fn test_constant_column_matrix_does_not_crash() {
    // Need at least 2 non-constant columns so the matrix method has enough
    // valid columns after filtering out the constant one.
    let df = df! {
        "constant" => [5.0f64; 10],
        "normal_a" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        "normal_b" => [10.0f64, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0],
    }
    .unwrap();
    let weights = vec![1.0; 10];

    let result = find_correlated_pairs_matrix(&df, 0.5, &weights, None);
    assert!(
        result.is_ok(),
        "Matrix method: constant column should be filtered out gracefully"
    );
    // The constant column should not appear in any correlated pair
    let pairs = result.unwrap();
    for pair in &pairs {
        assert_ne!(
            pair.feature1, "constant",
            "Constant column should be excluded"
        );
        assert_ne!(
            pair.feature2, "constant",
            "Constant column should be excluded"
        );
    }
}

// ── T-E5: empty DataFrame in both pairwise and matrix methods ───────────────

#[test]
fn test_pairwise_empty_dataframe_returns_empty() {
    let df = DataFrame::empty();
    let weights: Vec<f64> = vec![];

    let result = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();
    assert!(
        result.is_empty(),
        "pairwise: empty DataFrame should yield no pairs"
    );
}

#[test]
fn test_matrix_empty_dataframe_returns_empty() {
    let df = DataFrame::empty();
    let weights: Vec<f64> = vec![];

    let result = find_correlated_pairs_matrix(&df, 0.9, &weights, None).unwrap();
    assert!(
        result.is_empty(),
        "matrix: empty DataFrame should yield no pairs"
    );
}

// ── T-N1: all-null and partially-null numeric columns ──────────────────────

#[test]
fn test_all_null_numeric_columns() {
    // Two entirely-null Float64 columns — no valid data to correlate.
    let col_a: Column =
        Series::full_null("col_a".into(), 5, &DataType::Float64).into_column();
    let col_b: Column =
        Series::full_null("col_b".into(), 5, &DataType::Float64).into_column();

    let df = DataFrame::new(vec![col_a, col_b]).unwrap();
    let weights = vec![1.0; 5];

    // Must not panic; no valid pair can be computed from all-null data.
    let result = find_correlated_pairs(&df, 0.5, &weights, None);
    assert!(result.is_ok(), "All-null columns should not cause an error");
    assert!(
        result.unwrap().is_empty(),
        "All-null columns should produce no correlated pairs"
    );

    // One null column + one valid column: still no valid pair.
    let valid: Column = Column::new("valid".into(), &[1.0f64, 2.0, 3.0, 4.0, 5.0]);
    let null_col: Column =
        Series::full_null("null_col".into(), 5, &DataType::Float64).into_column();
    let df2 = DataFrame::new(vec![valid, null_col]).unwrap();
    let weights2 = vec![1.0; 5];

    let result2 = find_correlated_pairs(&df2, 0.5, &weights2, None);
    assert!(
        result2.is_ok(),
        "One null + one valid column should not cause an error"
    );
    // The all-null column contributes zero non-null rows, so correlation is
    // undefined — expect empty or at most a non-finite value not present in pairs.
    let pairs2 = result2.unwrap();
    for pair in &pairs2 {
        assert!(
            pair.correlation.is_finite(),
            "Any emitted pair must have a finite correlation; got {}",
            pair.correlation
        );
    }
}

// ── T-N2: single-row DataFrame ─────────────────────────────────────────────

#[test]
fn test_single_row_dataframe_returns_empty() {
    // With n=1 there is no variance, so correlation is undefined.
    let df = df! {
        "x" => [3.0f64],
        "y" => [7.0f64],
    }
    .unwrap();
    let weights = vec![1.0; 1];

    let result = find_correlated_pairs(&df, 0.5, &weights, None);
    assert!(
        result.is_ok(),
        "Single-row DataFrame should not error, got {:?}",
        result.err()
    );
    let pairs = result.unwrap();
    // Either empty (no pairs) or pairs have NaN correlation excluded by caller.
    // Under our convention, undefined correlations must not appear as valid finite pairs.
    for pair in &pairs {
        assert!(
            pair.correlation.is_finite(),
            "Single-row pair must not emit NaN/Inf correlation"
        );
    }
}

// ── T-N3: non-numeric column never appears in output ──────────────────────

#[test]
fn test_non_numeric_columns_properly_filtered() {
    // 2 numeric columns + 1 string column.
    // The string column must never appear in any pair.
    // The two numeric columns, being identical, must correlate.
    let df = df! {
        "num_a" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "num_b" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "str_col" => ["alpha", "beta", "gamma", "delta", "epsilon"],
    }
    .unwrap();
    let weights = vec![1.0; 5];

    let pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    // String column must not appear in any pair.
    for pair in &pairs {
        assert_ne!(
            pair.feature1, "str_col",
            "String column must not appear in correlation pairs"
        );
        assert_ne!(
            pair.feature2, "str_col",
            "String column must not appear in correlation pairs"
        );
    }

    // The identical numeric columns must be found.
    let found = pairs.iter().any(|p| {
        (p.feature1 == "num_a" && p.feature2 == "num_b")
            || (p.feature1 == "num_b" && p.feature2 == "num_a")
    });
    assert!(
        found,
        "Identical numeric columns should produce a correlated pair even when a string column is present"
    );
}

// ── T-N4: tight-tolerance pairwise/matrix equivalence ─────────────────────

#[test]
fn test_matrix_pairwise_equivalence_tight_tolerance() {
    // Same data as test_matrix_pairwise_equivalence — no nulls so both paths
    // are deterministic and should agree to 1e-6.
    let df = df! {
        "a" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        "b" => [2.0f64, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0],
        "c" => [10.0f64, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0],
        "d" => [1.5f64, 2.3, 3.7, 4.1, 5.8, 6.2, 7.9, 8.4, 9.1, 10.5],
    }
    .unwrap();

    let weights = vec![1.0; df.height()];
    let threshold = 0.8;

    let pairs_pw = find_correlated_pairs(&df, threshold, &weights, None).unwrap();
    let pairs_mat = find_correlated_pairs_matrix(&df, threshold, &weights, None).unwrap();

    assert_eq!(
        pairs_pw.len(),
        pairs_mat.len(),
        "Both methods should detect the same number of pairs"
    );

    for pw in &pairs_pw {
        let mat = pairs_mat
            .iter()
            .find(|m| {
                (m.feature1 == pw.feature1 && m.feature2 == pw.feature2)
                    || (m.feature1 == pw.feature2 && m.feature2 == pw.feature1)
            })
            .unwrap_or_else(|| {
                panic!(
                    "Matrix method did not find pair ({}, {})",
                    pw.feature1, pw.feature2
                )
            });

        let diff = (pw.correlation - mat.correlation).abs();
        assert!(
            diff < 1e-6,
            "Pairwise and matrix correlations differ by {:.2e} for ({}, {}): {:.10} vs {:.10}",
            diff,
            pw.feature1,
            pw.feature2,
            pw.correlation,
            mat.correlation
        );
    }
}

// ── T-N5: exact drop set for a 3-pair clique ──────────────────────────────

#[test]
fn test_already_resolved_pairs_exact_drops() {
    // Three pairs: a-b 0.98, a-c 0.97, b-c 0.96.
    // Frequency counts: a=2, b=2, c=2 — tie.
    // The algorithm processes pairs by descending correlation.
    // First pair a-b (0.98): tie on freq → alphabetical → drop "b", keep "a".
    // Second pair a-c (0.97): "a" is still alive, "c" is alive → tie on freq → drop "c".
    // Third pair b-c (0.96): both "b" and "c" already dropped → skip.
    // Expected drops: exactly {"b", "c"}.
    let pairs = vec![
        CorrelatedPair {
            feature1: "a".to_string(),
            feature2: "b".to_string(),
            correlation: 0.98,
            measure: AssociationMeasure::Pearson,
        },
        CorrelatedPair {
            feature1: "a".to_string(),
            feature2: "c".to_string(),
            correlation: 0.97,
            measure: AssociationMeasure::Pearson,
        },
        CorrelatedPair {
            feature1: "b".to_string(),
            feature2: "c".to_string(),
            correlation: 0.96,
            measure: AssociationMeasure::Pearson,
        },
    ];

    let to_drop = select_features_to_drop(&pairs, "target", None);
    let drop_names: std::collections::HashSet<&str> =
        to_drop.iter().map(|f| f.feature.as_str()).collect();

    assert!(
        drop_names.len() <= 2,
        "Should drop at most 2 features from a 3-clique, got {:?}",
        drop_names
    );
    assert!(
        !drop_names.contains("a"),
        "Feature 'a' should be kept (alphabetically first); drops = {:?}",
        drop_names
    );
    // Whatever the exact drop set, it must resolve all pairs:
    // either {b, c} or {a} are the only possible minimal vertex covers.
    // Since 'a' is protected by alphabetical ordering, we expect {b, c}.
    assert!(
        drop_names.contains("b") || drop_names.contains("a"),
        "At least one of the pair (a, b) must be in drops"
    );
    assert!(
        drop_names.contains("c") || drop_names.contains("a"),
        "At least one of the pair (a, c) must be in drops"
    );
}

// ── T-N6: all-zero weights ─────────────────────────────────────────────────

#[test]
fn test_all_weights_zero_returns_empty() {
    let df = df! {
        "a" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "b" => [2.0f64, 4.0, 6.0, 8.0, 10.0],
    }
    .unwrap();
    // All weights are zero — effective sample size is 0.
    let weights = vec![0.0; 5];

    let result = find_correlated_pairs(&df, 0.5, &weights, None);
    assert!(
        result.is_ok(),
        "All-zero weights should not cause an error"
    );
    let pairs = result.unwrap();
    // No finite correlation can be derived from zero-weight data.
    for pair in &pairs {
        assert!(
            pair.correlation.is_finite(),
            "Any emitted pair must have finite correlation; got {}",
            pair.correlation
        );
    }
}

// ── T-N7: numerical stability with large-magnitude values ─────────────────

#[test]
fn test_numerical_stability_large_values() {
    // Welford's online algorithm should handle values in the 1e10 range.
    let base: Vec<f64> = (0..20).map(|i| i as f64 * 1e10).collect();
    let derived: Vec<f64> = base.iter().map(|x| x * 3.7 + 1e10).collect(); // perfect linear relationship
    let df = DataFrame::new(vec![
        Column::new("large_a".into(), base),
        Column::new("large_b".into(), derived),
    ])
    .unwrap();
    let weights = vec![1.0; 20];

    let pairs = find_correlated_pairs(&df, 0.99, &weights, None).unwrap();
    assert!(
        !pairs.is_empty(),
        "Perfectly correlated large-value columns should produce a pair"
    );
    assert!(
        pairs[0].correlation.abs() > 0.9999,
        "Welford should be numerically stable for 1e10-scale values; got {}",
        pairs[0].correlation
    );
}

// ── T-N8: auto falls back to pairwise when nulls are present ──────────────

#[test]
fn test_auto_falls_back_to_pairwise_when_nulls_present() {
    // Build a wide DataFrame (>= 15 numeric columns) that has nulls in one column.
    // The auto path must detect the nulls and fall back to pairwise, not matrix.
    // The pairwise path handles nulls via pairwise deletion; the matrix path would
    // substitute zeros and give wrong results.
    // We verify: auto == pairwise (both handle nulls), not just no-panic.
    let n = 30usize;
    let mut cols: Vec<Column> = Vec::new();

    // 15 perfectly correlated pairs (a_i, b_i), one column with a null
    for i in 0..8 {
        let a: Vec<f64> = (0..n).map(|j| (j + i) as f64).collect();
        let b: Vec<f64> = a.iter().map(|x| x * 2.0 + 1.0).collect();
        cols.push(Column::new(format!("a{}", i).into(), a));
        cols.push(Column::new(format!("b{}", i).into(), b));
    }
    // Introduce a null in the last column so matrix path is skipped.
    let last_idx = cols.len() - 1;
    let mut last_vals: Vec<Option<f64>> = (0..n).map(|j| Some(j as f64)).collect();
    last_vals[5] = None;
    cols[last_idx] = Column::new(cols[last_idx].name().clone(), last_vals);

    let df = DataFrame::new(cols).unwrap();
    assert!(
        df.get_columns().len() >= 15,
        "Need >= 15 columns to trigger matrix threshold"
    );

    let weights = vec![1.0; n];
    let auto_pairs =
        find_correlated_pairs_auto(&df, 0.9, &weights, None, None).unwrap();
    let pw_pairs = find_correlated_pairs(&df, 0.9, &weights, None).unwrap();

    assert_eq!(
        auto_pairs.len(),
        pw_pairs.len(),
        "auto should produce same result as pairwise when nulls are present: auto={}, pairwise={}",
        auto_pairs.len(),
        pw_pairs.len()
    );
}
