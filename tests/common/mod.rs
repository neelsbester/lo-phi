//! Shared test utilities and fixture generators

use polars::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a simple test DataFrame with known characteristics for testing
/// 
/// This DataFrame includes:
/// - `target`: Binary target column (0/1)
/// - `feature_good`: Clean numeric feature
/// - `feature_corr`: Highly correlated with feature_good (correlation > 0.99)
/// - `feature_missing`: 80% missing values (should be dropped at 30% threshold)
/// - `feature_constant`: Zero variance (constant value)
/// - `feature_low_gini`: Low predictive power
pub fn create_test_dataframe() -> DataFrame {
    df! {
        "target" => [0i32, 1, 0, 1, 0, 1, 0, 1, 0, 1],
        "feature_good" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        "feature_corr" => [1.1f64, 2.1, 3.1, 4.1, 5.1, 6.1, 7.1, 8.1, 9.1, 10.1], // Highly correlated with feature_good
        "feature_missing" => [Some(1.0f64), None, None, None, None, None, None, None, None, Some(10.0)], // 80% missing
        "feature_constant" => [5.0f64; 10], // Zero variance
        "feature_low_gini" => [1.0f64, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0], // Low predictive power
    }.unwrap()
}

/// Create a larger test DataFrame for performance/stress tests
pub fn create_large_test_dataframe(rows: usize, cols: usize) -> DataFrame {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    
    let mut columns: Vec<Column> = Vec::with_capacity(cols + 1);
    
    // Target column
    let target: Vec<i32> = (0..rows).map(|_| rng.gen_range(0..2)).collect();
    columns.push(Column::new("target".into(), target));
    
    // Feature columns
    for i in 0..cols {
        let values: Vec<f64> = (0..rows).map(|_| rng.gen::<f64>()).collect();
        columns.push(Column::new(format!("feature_{}", i).into(), values));
    }
    
    DataFrame::new(columns).unwrap()
}

/// Create a DataFrame with specific missing value patterns
pub fn create_missing_test_dataframe() -> DataFrame {
    df! {
        "col_complete" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "col_20pct_missing" => [Some(1.0f64), None, Some(3.0), Some(4.0), Some(5.0)], // 20% missing
        "col_40pct_missing" => [Some(1.0f64), Some(2.0), None, None, Some(5.0)], // 40% missing
        "col_all_missing" => [None::<f64>, None, None, None, None], // 100% missing
        "target" => [0i32, 1, 0, 1, 0],
    }.unwrap()
}

/// Create a DataFrame with known correlation patterns
pub fn create_correlation_test_dataframe() -> DataFrame {
    df! {
        "target" => [0i32, 1, 0, 1, 0, 1, 0, 1, 0, 1],
        "a" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        "b" => [2.0f64, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0], // Perfectly correlated with a (b = 2*a)
        "c" => [10.0f64, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0], // Negatively correlated with a
        "d" => [5.0f64, 1.0, 8.0, 2.0, 9.0, 3.0, 7.0, 4.0, 6.0, 0.0], // Uncorrelated random
    }.unwrap()
}

/// Create a temporary directory with a test CSV file
pub fn create_temp_csv(df: &mut DataFrame) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("test_data.csv");
    
    let mut file = std::fs::File::create(&csv_path).unwrap();
    CsvWriter::new(&mut file).finish(df).unwrap();
    
    (temp_dir, csv_path)
}

/// Create a temporary directory with a test Parquet file
pub fn create_temp_parquet(df: &mut DataFrame) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let parquet_path = temp_dir.path().join("test_data.parquet");
    
    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(df).unwrap();
    
    (temp_dir, parquet_path)
}

/// Assert that a DataFrame has expected shape
pub fn assert_shape(df: &DataFrame, expected_rows: usize, expected_cols: usize) {
    let (rows, cols) = df.shape();
    assert_eq!(rows, expected_rows, "Row count mismatch: expected {}, got {}", expected_rows, rows);
    assert_eq!(cols, expected_cols, "Column count mismatch: expected {}, got {}", expected_cols, cols);
}

/// Assert that a DataFrame contains specific columns
pub fn assert_has_columns(df: &DataFrame, expected_cols: &[&str]) {
    let actual_cols: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    for col in expected_cols {
        assert!(
            actual_cols.contains(&col.to_string()),
            "Missing expected column: '{}'. Actual columns: {:?}",
            col,
            actual_cols
        );
    }
}

/// Assert that a DataFrame does NOT contain specific columns
pub fn assert_missing_columns(df: &DataFrame, unexpected_cols: &[&str]) {
    let actual_cols: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    for col in unexpected_cols {
        assert!(
            !actual_cols.contains(&col.to_string()),
            "Unexpected column still present: '{}'",
            col
        );
    }
}

/// Create a minimal binary target DataFrame for IV/Gini tests
pub fn create_binary_target_dataframe() -> DataFrame {
    df! {
        "target" => [0i32, 0, 0, 0, 0, 1, 1, 1, 1, 1,
                     0, 0, 0, 0, 0, 1, 1, 1, 1, 1], // 50/50 split
        "predictive_feature" => [1.0f64, 1.0, 1.0, 2.0, 2.0, 8.0, 9.0, 9.0, 10.0, 10.0,
                                  1.5, 1.5, 2.0, 2.5, 3.0, 7.0, 8.0, 8.5, 9.0, 9.5], // Good separator
        "random_feature" => [5.0f64, 8.0, 2.0, 9.0, 1.0, 3.0, 7.0, 4.0, 6.0, 0.0,
                             4.0, 6.0, 8.0, 1.0, 9.0, 2.0, 5.0, 7.0, 3.0, 0.0], // Random noise
    }.unwrap()
}

