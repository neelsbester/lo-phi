//! Unit tests for missing value analysis

use lophi::pipeline::{analyze_missing_values, get_features_above_threshold};
use polars::prelude::*;

#[path = "common/mod.rs"]
mod common;

#[test]
fn test_analyze_missing_values_basic() {
    let df = df! {
        "col_complete" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "col_partial_missing" => [Some(1.0f64), Some(2.0), None, None, Some(5.0)],
        "col_all_missing" => [None::<f64>, None, None, None, None],
    }.unwrap();
    let weights = vec![1.0; 5];

    let ratios = analyze_missing_values(&df, &weights, None).unwrap();
    
    // Convert to HashMap for easier lookup
    let ratio_map: std::collections::HashMap<_, _> = ratios.into_iter().collect();
    
    // col_complete: 0% missing
    assert!(
        (ratio_map["col_complete"] - 0.0).abs() < 0.001,
        "col_complete should have 0% missing, got {}",
        ratio_map["col_complete"]
    );
    
    // col_partial_missing: 2/5 = 40% missing
    assert!(
        (ratio_map["col_partial_missing"] - 0.4).abs() < 0.001,
        "col_partial_missing should have 40% missing, got {}",
        ratio_map["col_partial_missing"]
    );
    
    // col_all_missing: 100% missing
    assert!(
        (ratio_map["col_all_missing"] - 1.0).abs() < 0.001,
        "col_all_missing should have 100% missing, got {}",
        ratio_map["col_all_missing"]
    );
}

#[test]
fn test_analyze_missing_values_sorted_descending() {
    let df = common::create_missing_test_dataframe();
    let weights = vec![1.0; df.height()];

    let ratios = analyze_missing_values(&df, &weights, None).unwrap();
    
    // Verify sorted descending by missing ratio
    for i in 0..ratios.len() - 1 {
        assert!(
            ratios[i].1 >= ratios[i + 1].1,
            "Ratios should be sorted descending: {} >= {}",
            ratios[i].1,
            ratios[i + 1].1
        );
    }
}

#[test]
fn test_get_features_above_threshold() {
    let ratios = vec![
        ("feature_a".to_string(), 0.1),  // Below threshold
        ("feature_b".to_string(), 0.35), // Above threshold
        ("target".to_string(), 0.5),     // Above but is target (protected)
        ("feature_c".to_string(), 0.9),  // Above threshold
    ];
    
    let to_drop = get_features_above_threshold(&ratios, 0.3, "target");
    
    assert_eq!(to_drop.len(), 2, "Should drop exactly 2 features");
    assert!(to_drop.contains(&"feature_b".to_string()), "Should drop feature_b (35% missing)");
    assert!(to_drop.contains(&"feature_c".to_string()), "Should drop feature_c (90% missing)");
    assert!(!to_drop.contains(&"target".to_string()), "Should NOT drop target column");
    assert!(!to_drop.contains(&"feature_a".to_string()), "Should NOT drop feature_a (10% missing)");
}

#[test]
fn test_get_features_threshold_boundary() {
    let ratios = vec![
        ("exactly_at_threshold".to_string(), 0.3), // Exactly at threshold (should NOT drop)
        ("just_above".to_string(), 0.301),         // Just above threshold (should drop)
    ];
    
    let to_drop = get_features_above_threshold(&ratios, 0.3, "target");
    
    assert_eq!(to_drop.len(), 1);
    assert!(to_drop.contains(&"just_above".to_string()));
    assert!(!to_drop.contains(&"exactly_at_threshold".to_string()), 
            "Features exactly at threshold should NOT be dropped");
}

#[test]
fn test_empty_dataframe() {
    let df = DataFrame::empty();
    let weights: Vec<f64> = vec![];
    let ratios = analyze_missing_values(&df, &weights, None).unwrap();
    assert!(ratios.is_empty(), "Empty DataFrame should produce empty ratios");
}

#[test]
fn test_no_missing_values() {
    let df = df! {
        "a" => [1i32, 2, 3],
        "b" => [4i32, 5, 6],
        "c" => [7i32, 8, 9],
    }.unwrap();
    let weights = vec![1.0; 3];

    let ratios = analyze_missing_values(&df, &weights, None).unwrap();
    
    for (col_name, ratio) in &ratios {
        assert_eq!(
            *ratio, 0.0,
            "Column '{}' should have 0% missing, got {}",
            col_name, ratio
        );
    }
}

#[test]
fn test_all_columns_above_threshold() {
    let ratios = vec![
        ("col_a".to_string(), 0.5),
        ("col_b".to_string(), 0.6),
        ("col_c".to_string(), 0.7),
    ];
    
    let to_drop = get_features_above_threshold(&ratios, 0.3, "target");
    
    assert_eq!(to_drop.len(), 3, "All columns should be dropped");
}

#[test]
fn test_no_columns_above_threshold() {
    let ratios = vec![
        ("col_a".to_string(), 0.1),
        ("col_b".to_string(), 0.2),
        ("col_c".to_string(), 0.25),
    ];
    
    let to_drop = get_features_above_threshold(&ratios, 0.3, "target");
    
    assert!(to_drop.is_empty(), "No columns should be dropped");
}

#[test]
fn test_with_integer_columns() {
    let df = df! {
        "int_col" => [Some(1i32), None, Some(3), Some(4), None],
        "float_col" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
    }.unwrap();
    let weights = vec![1.0; 5];

    let ratios = analyze_missing_values(&df, &weights, None).unwrap();
    let ratio_map: std::collections::HashMap<_, _> = ratios.into_iter().collect();
    
    // int_col: 2/5 = 40% missing
    assert!(
        (ratio_map["int_col"] - 0.4).abs() < 0.001,
        "int_col should have 40% missing"
    );
    
    // float_col: 0% missing
    assert!(
        (ratio_map["float_col"] - 0.0).abs() < 0.001,
        "float_col should have 0% missing"
    );
}

#[test]
fn test_weighted_missing_ratio() {
    // Test that weighted missing ratio is correctly calculated
    // Row 0: value=1.0, weight=1.0 (not missing)
    // Row 1: value=null, weight=3.0 (missing, weight 3.0)
    // Row 2: value=3.0, weight=1.0 (not missing)
    // Row 3: value=null, weight=1.0 (missing, weight 1.0)
    // Row 4: value=5.0, weight=4.0 (not missing)
    //
    // Total weight = 1+3+1+1+4 = 10
    // Weighted null count = 3+1 = 4
    // Weighted missing ratio = 4/10 = 0.4
    let df = df! {
        "feature" => [Some(1.0f64), None, Some(3.0), None, Some(5.0)],
    }.unwrap();
    let weights = vec![1.0, 3.0, 1.0, 1.0, 4.0];

    let ratios = analyze_missing_values(&df, &weights, None).unwrap();
    let ratio_map: std::collections::HashMap<_, _> = ratios.into_iter().collect();
    
    assert!(
        (ratio_map["feature"] - 0.4).abs() < 0.001,
        "Weighted missing ratio should be 0.4, got {}",
        ratio_map["feature"]
    );
}

#[test]
fn test_weighted_missing_ratio_vs_unweighted() {
    // With uniform weights of 1.0, weighted ratio should equal unweighted ratio
    // 2 nulls out of 5 rows = 40% missing
    let df = df! {
        "feature" => [Some(1.0f64), None, Some(3.0), None, Some(5.0)],
    }.unwrap();
    
    // Uniform weights
    let uniform_weights = vec![1.0; 5];
    let ratios_uniform = analyze_missing_values(&df, &uniform_weights, None).unwrap();
    let ratio_uniform = ratios_uniform.iter().find(|(n, _)| n == "feature").unwrap().1;
    
    // Non-uniform weights that give same result as unweighted
    // If all nulls have same total weight as all non-nulls proportionally
    // Nulls at index 1,3: if we want 40% missing, nulls need 40% of total weight
    let weighted_weights = vec![2.0, 2.0, 2.0, 2.0, 2.0]; // Still uniform, just scaled
    let ratios_weighted = analyze_missing_values(&df, &weighted_weights, None).unwrap();
    let ratio_weighted = ratios_weighted.iter().find(|(n, _)| n == "feature").unwrap().1;
    
    assert!(
        (ratio_uniform - ratio_weighted).abs() < 0.001,
        "Uniform weights should produce same ratio regardless of scale: {} vs {}",
        ratio_uniform, ratio_weighted
    );
}

#[test]
fn test_weight_column_excluded_from_analysis() {
    // When weight_column is specified, it should be excluded from the results
    let df = df! {
        "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "weight" => [1.0f64, 2.0, 0.5, 1.5, 1.0],
    }.unwrap();
    let weights = vec![1.0, 2.0, 0.5, 1.5, 1.0];

    // Without exclusion - weight column should be in results
    let ratios_included = analyze_missing_values(&df, &weights, None).unwrap();
    assert!(
        ratios_included.iter().any(|(name, _)| name == "weight"),
        "Weight column should be included when not excluded"
    );

    // With exclusion - weight column should NOT be in results
    let ratios_excluded = analyze_missing_values(&df, &weights, Some("weight")).unwrap();
    assert!(
        !ratios_excluded.iter().any(|(name, _)| name == "weight"),
        "Weight column should be excluded when specified"
    );
    assert_eq!(ratios_excluded.len(), 1, "Should only have 'feature' column");
}

