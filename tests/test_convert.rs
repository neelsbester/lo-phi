//! Tests for CSV to Parquet conversion functionality

mod common;

use lophi::cli::run_convert;
use polars::prelude::*;
use tempfile::TempDir;

/// Helper to create a test CSV file with specific data types
fn create_test_csv(temp_dir: &TempDir, name: &str, df: &mut DataFrame) -> std::path::PathBuf {
    let csv_path = temp_dir.path().join(name);
    let mut file = std::fs::File::create(&csv_path).unwrap();
    CsvWriter::new(&mut file).finish(df).unwrap();
    csv_path
}

#[test]
fn test_basic_csv_to_parquet_conversion() {
    // Create a simple test DataFrame
    let mut df = df! {
        "id" => [1i32, 2, 3, 4, 5],
        "value" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "target" => [0i32, 1, 0, 1, 0],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let csv_path = create_test_csv(&temp_dir, "test.csv", &mut df);
    let parquet_path = temp_dir.path().join("test.parquet");

    // Convert CSV to Parquet
    run_convert(&csv_path, Some(&parquet_path), 1000).unwrap();

    // Verify the Parquet file exists and has correct shape
    assert!(parquet_path.exists(), "Parquet file should be created");

    let result_df = LazyFrame::scan_parquet(&parquet_path, Default::default())
        .unwrap()
        .collect()
        .unwrap();

    assert_eq!(result_df.shape(), (5, 3));
    assert!(result_df.column("id").is_ok());
    assert!(result_df.column("value").is_ok());
    assert!(result_df.column("target").is_ok());
}

#[test]
fn test_conversion_preserves_data_types() {
    // Create a DataFrame with various data types
    let mut df = df! {
        "int_col" => [1i32, 2, 3, 4, 5],
        "float_col" => [1.5f64, 2.5, 3.5, 4.5, 5.5],
        "target" => [0i32, 1, 0, 1, 0],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let csv_path = create_test_csv(&temp_dir, "types_test.csv", &mut df);
    let parquet_path = temp_dir.path().join("types_test.parquet");

    run_convert(&csv_path, Some(&parquet_path), 1000).unwrap();

    let result_df = LazyFrame::scan_parquet(&parquet_path, Default::default())
        .unwrap()
        .collect()
        .unwrap();

    // Check that numeric types are preserved (may be inferred as Int64/Float64)
    let int_col = result_df.column("int_col").unwrap();
    let float_col = result_df.column("float_col").unwrap();

    assert!(
        int_col.dtype().is_integer() || int_col.dtype().is_float(),
        "int_col should be numeric"
    );
    assert!(
        float_col.dtype().is_float(),
        "float_col should be float"
    );
}

#[test]
fn test_conversion_with_binary_target_preserved() {
    // Create a DataFrame with binary target (0/1)
    let mut df = df! {
        "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        "target" => [0i32, 1, 0, 1, 0, 1, 0, 1, 0, 1],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let csv_path = create_test_csv(&temp_dir, "binary_target.csv", &mut df);
    let parquet_path = temp_dir.path().join("binary_target.parquet");

    run_convert(&csv_path, Some(&parquet_path), 1000).unwrap();

    let result_df = LazyFrame::scan_parquet(&parquet_path, Default::default())
        .unwrap()
        .collect()
        .unwrap();

    // Verify target column values are 0 and 1
    let target_col = result_df.column("target").unwrap();
    let unique = target_col.unique().unwrap();
    let unique_sorted: Vec<i64> = unique
        .cast(&DataType::Int64)
        .unwrap()
        .i64()
        .unwrap()
        .into_no_null_iter()
        .collect();

    assert!(
        unique_sorted.iter().all(|&v| v == 0 || v == 1),
        "Target values should be 0 or 1, got: {:?}",
        unique_sorted
    );
}

#[test]
fn test_conversion_auto_output_path() {
    let mut df = df! {
        "a" => [1i32, 2, 3],
        "b" => [4.0f64, 5.0, 6.0],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let csv_path = create_test_csv(&temp_dir, "auto_output.csv", &mut df);

    // Convert without explicit output path
    run_convert(&csv_path, None, 1000).unwrap();

    // Should create parquet with same base name
    let expected_parquet = temp_dir.path().join("auto_output.parquet");
    assert!(
        expected_parquet.exists(),
        "Auto-generated parquet file should exist at {:?}",
        expected_parquet
    );
}

#[test]
fn test_conversion_with_missing_values() {
    // Create a DataFrame with missing values
    let mut df = df! {
        "complete" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "with_nulls" => [Some(1.0f64), None, Some(3.0), None, Some(5.0)],
        "target" => [0i32, 1, 0, 1, 0],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let csv_path = create_test_csv(&temp_dir, "nulls_test.csv", &mut df);
    let parquet_path = temp_dir.path().join("nulls_test.parquet");

    run_convert(&csv_path, Some(&parquet_path), 1000).unwrap();

    let result_df = LazyFrame::scan_parquet(&parquet_path, Default::default())
        .unwrap()
        .collect()
        .unwrap();

    // Verify null counts are preserved
    let with_nulls_col = result_df.column("with_nulls").unwrap();
    assert_eq!(with_nulls_col.null_count(), 2, "Null count should be preserved");
}

#[test]
fn test_conversion_produces_valid_parquet() {
    // Create a DataFrame with multiple columns to test conversion
    let n = 1000;
    let values: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let targets: Vec<i32> = (0..n).map(|i| (i % 2) as i32).collect();

    let mut df = df! {
        "feature" => values,
        "target" => targets,
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let csv_path = create_test_csv(&temp_dir, "compression_test.csv", &mut df);
    let parquet_path = temp_dir.path().join("compression_test.parquet");

    run_convert(&csv_path, Some(&parquet_path), 1000).unwrap();

    // Verify the Parquet file is valid and readable
    let result_df = LazyFrame::scan_parquet(&parquet_path, Default::default())
        .unwrap()
        .collect()
        .unwrap();

    assert_eq!(result_df.shape(), (n, 2));
    
    // Verify data integrity
    let feature_col = result_df.column("feature").unwrap();
    let first_val = feature_col.get(0).unwrap();
    let last_val = feature_col.get(n - 1).unwrap();
    
    // Check first and last values match expected
    assert!(matches!(first_val, AnyValue::Float64(v) if (v - 0.0).abs() < 0.01));
    assert!(matches!(last_val, AnyValue::Float64(v) if (v - 999.0).abs() < 0.01));
}

#[test]
fn test_conversion_with_many_columns() {
    // Create a DataFrame with many columns
    let mut columns: Vec<Column> = Vec::new();

    for i in 0..50 {
        let values: Vec<f64> = vec![i as f64; 10];
        columns.push(Column::new(format!("col_{}", i).into(), values));
    }
    columns.push(Column::new("target".into(), vec![0i32, 1, 0, 1, 0, 1, 0, 1, 0, 1]));

    let mut df = DataFrame::new(columns).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let csv_path = create_test_csv(&temp_dir, "wide_test.csv", &mut df);
    let parquet_path = temp_dir.path().join("wide_test.parquet");

    run_convert(&csv_path, Some(&parquet_path), 1000).unwrap();

    let result_df = LazyFrame::scan_parquet(&parquet_path, Default::default())
        .unwrap()
        .collect()
        .unwrap();

    assert_eq!(result_df.shape(), (10, 51), "Should have 10 rows and 51 columns");
}

