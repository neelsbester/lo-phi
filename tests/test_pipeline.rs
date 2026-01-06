//! Integration tests for the full reduction pipeline

use lophi::pipeline::*;
use polars::prelude::*;

#[path = "common/mod.rs"]
mod common;

use common::*;

#[test]
fn test_full_pipeline_reduces_features() {
    // Create test data with known drop candidates
    let mut df = create_test_dataframe();
    let (_temp_dir, csv_path) = create_temp_csv(&mut df);

    // Load
    let (mut df, _rows, initial_cols, _mem) = load_dataset_with_progress(&csv_path, 100).unwrap();
    let weights = vec![1.0; df.height()];

    // Step 1: Missing value analysis (should drop feature_missing at 80% missing)
    let missing_ratios = analyze_missing_values(&df, &weights, None).unwrap();
    let missing_drops = get_features_above_threshold(&missing_ratios, 0.3, "target");

    assert!(
        missing_drops.contains(&"feature_missing".to_string()),
        "Should identify feature_missing for dropping (80% missing)"
    );

    df = df.drop_many(&missing_drops);

    // Step 2: Correlation analysis (should drop one of feature_good/feature_corr)
    let weights = vec![1.0; df.height()];
    let pairs = find_correlated_pairs(&df, 0.95, &weights, None).unwrap();
    let corr_drops = select_features_to_drop(&pairs, "target");
    df = df.drop_many(&corr_drops);

    // Verify reduction occurred
    let (_, final_cols) = df.shape();
    assert!(
        final_cols < initial_cols,
        "Expected feature reduction: initial={}, final={}",
        initial_cols,
        final_cols
    );

    // Target should always be preserved
    assert_has_columns(&df, &["target"]);
}

#[test]
fn test_pipeline_preserves_target_column() {
    let mut df = df! {
        "target" => [0i32, 1, 0, 1, 0, 1, 0, 1, 0, 1],
        "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
    }.unwrap();

    let (_temp_dir, csv_path) = create_temp_csv(&mut df);
    let (df, _, _, _) = load_dataset_with_progress(&csv_path, 100).unwrap();

    // Target should exist after loading
    assert_has_columns(&df, &["target"]);
}

#[test]
fn test_pipeline_handles_all_numeric_dataset() {
    let mut df = df! {
        "target" => [0i32, 1, 0, 1, 0, 1, 0, 1, 0, 1],
        "f1" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        "f2" => [10.0f64, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0],
    }.unwrap();

    let (_temp_dir, parquet_path) = create_temp_parquet(&mut df);
    let (df, rows, cols, _) = load_dataset_with_progress(&parquet_path, 100).unwrap();
    let weights = vec![1.0; df.height()];

    assert_eq!(rows, 10);
    assert_eq!(cols, 3);

    // All numeric - missing analysis should find no issues
    let missing = analyze_missing_values(&df, &weights, None).unwrap();
    assert!(
        missing.iter().all(|(_, ratio)| *ratio == 0.0),
        "All-numeric complete dataset should have no missing values"
    );
}

#[test]
fn test_pipeline_with_no_reductions_needed() {
    // Clean data with truly uncorrelated features
    let mut df = df! {
        "target" => [0i32, 1, 0, 1, 0, 1, 0, 1, 0, 1],
        "independent_a" => [1.0f64, 5.0, 2.0, 8.0, 3.0, 9.0, 4.0, 6.0, 7.0, 0.0],
        "independent_b" => [9.0f64, 3.0, 7.0, 1.0, 6.0, 2.0, 8.0, 5.0, 0.0, 4.0],
    }.unwrap();

    let (_temp_dir, csv_path) = create_temp_csv(&mut df);
    let (df, _, initial_cols, _) = load_dataset_with_progress(&csv_path, 100).unwrap();
    let weights = vec![1.0; df.height()];

    // Missing - none above 30%
    let missing_ratios = analyze_missing_values(&df, &weights, None).unwrap();
    let missing_drops = get_features_above_threshold(&missing_ratios, 0.3, "target");
    assert!(missing_drops.is_empty(), "No features should be dropped for missing values");

    // Correlation - check that no pairs exceed 0.95 threshold among non-target columns
    let pairs = find_correlated_pairs(&df, 0.95, &weights, None).unwrap();
    // Filter to only pairs between our independent features
    let feature_pairs: Vec<_> = pairs.iter()
        .filter(|p| p.feature1 != "target" && p.feature2 != "target")
        .collect();
    assert!(feature_pairs.is_empty(), "Independent features should not be correlated above 0.95");

    // Final shape unchanged (no drops for missing)
    let missing_drop_count = get_features_above_threshold(&missing_ratios, 0.3, "target").len();
    assert_eq!(missing_drop_count, 0, "Clean data should have no missing-based reductions");
}

#[test]
fn test_pipeline_missing_then_correlation() {
    // Test that steps work correctly in sequence
    let mut df = create_test_dataframe();
    let (_temp_dir, parquet_path) = create_temp_parquet(&mut df);

    let (mut df, _, _, _) = load_dataset_with_progress(&parquet_path, 100).unwrap();
    let weights = vec![1.0; df.height()];

    // Record column count after each step
    let cols_initial = df.width();

    // Step 1: Missing
    let missing_ratios = analyze_missing_values(&df, &weights, None).unwrap();
    let missing_drops = get_features_above_threshold(&missing_ratios, 0.3, "target");
    let missing_drop_count = missing_drops.len();
    df = df.drop_many(&missing_drops);

    assert_eq!(
        df.width(),
        cols_initial - missing_drop_count,
        "Width should decrease by missing drop count"
    );

    // Step 2: Correlation (should still work after missing step)
    let weights = vec![1.0; df.height()];
    let pairs = find_correlated_pairs(&df, 0.95, &weights, None).unwrap();
    let corr_drops = select_features_to_drop(&pairs, "target");
    let corr_drop_count = corr_drops.len();
    df = df.drop_many(&corr_drops);

    assert_eq!(
        df.width(),
        cols_initial - missing_drop_count - corr_drop_count,
        "Width should decrease by total drop count"
    );

    // Target always preserved
    assert_has_columns(&df, &["target"]);
}

#[test]
fn test_drop_many_removes_correct_columns() {
    let df = df! {
        "keep_me" => [1, 2, 3],
        "drop_me_1" => [4, 5, 6],
        "drop_me_2" => [7, 8, 9],
        "also_keep" => [10, 11, 12],
    }.unwrap();

    let to_drop = vec!["drop_me_1".to_string(), "drop_me_2".to_string()];
    let result = df.drop_many(&to_drop);

    assert_eq!(result.width(), 2);
    assert_has_columns(&result, &["keep_me", "also_keep"]);
    assert_missing_columns(&result, &["drop_me_1", "drop_me_2"]);
}

#[test]
fn test_pipeline_with_highly_correlated_pair() {
    let df = create_correlation_test_dataframe();
    let weights = vec![1.0; df.height()];

    // Columns a and b are perfectly correlated (b = 2*a)
    // Columns a and c are negatively correlated
    let pairs = find_correlated_pairs(&df, 0.95, &weights, None).unwrap();

    assert!(!pairs.is_empty(), "Should find correlated pairs");

    let to_drop = select_features_to_drop(&pairs, "target");

    // The algorithm should drop features to resolve correlations
    // Since 'a' is involved in multiple correlations (with b and c),
    // it's likely to be dropped due to higher frequency
    assert!(
        !to_drop.is_empty(),
        "Should drop at least one feature to resolve correlations"
    );

    // Verify that not ALL features are dropped - we should keep some
    let keeps_some = !to_drop.contains(&"a".to_string())
        || !to_drop.contains(&"b".to_string())
        || !to_drop.contains(&"c".to_string());
    assert!(
        keeps_some,
        "Should not drop all correlated features, got drops: {:?}",
        to_drop
    );

    // Verify target is never dropped
    assert!(
        !to_drop.contains(&"target".to_string()),
        "Target should never be dropped"
    );
}

#[test]
fn test_pipeline_large_dataset() {
    // Test with a larger dataset to ensure no performance issues
    let mut df = create_large_test_dataframe(500, 20);
    let (_temp_dir, parquet_path) = create_temp_parquet(&mut df);

    let (df, rows, cols, _) = load_dataset_with_progress(&parquet_path, 100).unwrap();
    let weights = vec![1.0; df.height()];

    assert_eq!(rows, 500);
    assert_eq!(cols, 21); // 20 features + 1 target

    // Should complete without errors
    let missing_ratios = analyze_missing_values(&df, &weights, None).unwrap();
    assert_eq!(missing_ratios.len(), 21);

    let pairs = find_correlated_pairs(&df, 0.95, &weights, None).unwrap();
    // Random data unlikely to have high correlations, but this shouldn't error
    let _ = select_features_to_drop(&pairs, "target");
}

#[test]
fn test_csv_and_parquet_produce_same_results() {
    let mut df = create_test_dataframe();

    // Save as both formats
    let (_temp_dir_csv, csv_path) = create_temp_csv(&mut df.clone());
    let (_temp_dir_parquet, parquet_path) = create_temp_parquet(&mut df);

    // Load both
    let (df_csv, rows_csv, cols_csv, _) = load_dataset_with_progress(&csv_path, 100).unwrap();
    let (df_parquet, rows_parquet, cols_parquet, _) = load_dataset_with_progress(&parquet_path, 100).unwrap();

    // Same dimensions
    assert_eq!(rows_csv, rows_parquet);
    assert_eq!(cols_csv, cols_parquet);

    // Same columns
    assert_eq!(df_csv.get_column_names(), df_parquet.get_column_names());

    // Same missing analysis results
    let weights_csv = vec![1.0; df_csv.height()];
    let weights_parquet = vec![1.0; df_parquet.height()];
    let missing_csv = analyze_missing_values(&df_csv, &weights_csv, None).unwrap();
    let missing_parquet = analyze_missing_values(&df_parquet, &weights_parquet, None).unwrap();

    for ((name_csv, ratio_csv), (name_parquet, ratio_parquet)) in
        missing_csv.iter().zip(missing_parquet.iter())
    {
        assert_eq!(name_csv, name_parquet);
        assert!(
            (ratio_csv - ratio_parquet).abs() < 0.001,
            "Missing ratios should match between CSV and Parquet"
        );
    }
}
