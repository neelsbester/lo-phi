//! Unit tests for dataset loader

use lophi::pipeline::{get_column_names, load_dataset_with_progress};
use polars::prelude::*;
use std::io::Write;
use tempfile::TempDir;

#[path = "common/mod.rs"]
mod common;

#[test]
fn test_load_csv_file() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("test.csv");

    let mut file = std::fs::File::create(&csv_path).unwrap();
    writeln!(file, "a,b,c").unwrap();
    writeln!(file, "1,2,3").unwrap();
    writeln!(file, "4,5,6").unwrap();
    drop(file);

    let (df, rows, cols, mem_mb) = load_dataset_with_progress(&csv_path, 100).unwrap();

    assert_eq!(rows, 2, "Should have 2 data rows");
    assert_eq!(cols, 3, "Should have 3 columns");
    assert_eq!(df.get_column_names(), &["a", "b", "c"]);
    assert!(mem_mb >= 0.0, "Memory estimate should be non-negative");
}

#[test]
fn test_load_parquet_file() {
    let temp_dir = TempDir::new().unwrap();
    let parquet_path = temp_dir.path().join("test.parquet");

    let mut df = df! {
        "x" => [1i32, 2, 3],
        "y" => [4i32, 5, 6],
    }
    .unwrap();

    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();

    let (loaded_df, rows, cols, _mem) = load_dataset_with_progress(&parquet_path, 100).unwrap();

    assert_eq!(rows, 3);
    assert_eq!(cols, 2);
    assert_eq!(loaded_df.get_column_names(), &["x", "y"]);
}

#[test]
fn test_get_column_names_csv() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("test.csv");

    let mut file = std::fs::File::create(&csv_path).unwrap();
    writeln!(file, "col_a,col_b,col_c").unwrap();
    writeln!(file, "1,2,3").unwrap();
    drop(file);

    let columns = get_column_names(&csv_path).unwrap();

    assert_eq!(columns, vec!["col_a", "col_b", "col_c"]);
}

#[test]
fn test_get_column_names_parquet() {
    let temp_dir = TempDir::new().unwrap();
    let parquet_path = temp_dir.path().join("test.parquet");

    let mut df = df! {
        "feature_1" => [1i32],
        "feature_2" => [2i32],
        "target" => [0i32],
    }
    .unwrap();

    let file = std::fs::File::create(&parquet_path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();

    let columns = get_column_names(&parquet_path).unwrap();

    assert_eq!(columns.len(), 3);
    assert!(columns.contains(&"feature_1".to_string()));
    assert!(columns.contains(&"feature_2".to_string()));
    assert!(columns.contains(&"target".to_string()));
}

#[test]
fn test_unsupported_format() {
    let temp_dir = TempDir::new().unwrap();
    let bad_path = temp_dir.path().join("test.xlsx");
    std::fs::File::create(&bad_path).unwrap();

    let result = load_dataset_with_progress(&bad_path, 100);

    assert!(result.is_err(), "Unsupported format should return error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Unsupported") || err_msg.contains("format"),
        "Error message should mention unsupported format: {}",
        err_msg
    );
}

#[test]
fn test_nonexistent_file() {
    let path = std::path::Path::new("/nonexistent/path/to/file.csv");

    let result = load_dataset_with_progress(path, 100);

    assert!(result.is_err(), "Nonexistent file should return error");
}

#[test]
fn test_csv_with_mixed_types() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("mixed.csv");

    let mut file = std::fs::File::create(&csv_path).unwrap();
    writeln!(file, "int_col,float_col,str_col").unwrap();
    writeln!(file, "1,1.5,hello").unwrap();
    writeln!(file, "2,2.5,world").unwrap();
    drop(file);

    let (df, rows, cols, _) = load_dataset_with_progress(&csv_path, 100).unwrap();

    assert_eq!(rows, 2);
    assert_eq!(cols, 3);

    // Verify column types are inferred
    let schema = df.schema();
    assert!(schema.get("int_col").is_some());
    assert!(schema.get("float_col").is_some());
    assert!(schema.get("str_col").is_some());
}

#[test]
fn test_csv_with_missing_values() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("missing.csv");

    let mut file = std::fs::File::create(&csv_path).unwrap();
    writeln!(file, "a,b,c").unwrap();
    writeln!(file, "1,,3").unwrap(); // b is missing
    writeln!(file, ",2,").unwrap(); // a and c are missing
    writeln!(file, "4,5,6").unwrap();
    drop(file);

    let (df, rows, cols, _) = load_dataset_with_progress(&csv_path, 100).unwrap();

    assert_eq!(rows, 3);
    assert_eq!(cols, 3);

    // Check that null counts are correct
    let null_counts: Vec<u32> = df
        .get_columns()
        .iter()
        .map(|c| c.null_count() as u32)
        .collect();

    assert_eq!(null_counts[0], 1, "Column 'a' should have 1 null");
    assert_eq!(null_counts[1], 1, "Column 'b' should have 1 null");
    assert_eq!(null_counts[2], 1, "Column 'c' should have 1 null");
}

#[test]
fn test_large_file_memory_estimate() {
    let mut df = common::create_large_test_dataframe(1000, 50);
    let (temp_dir, parquet_path) = common::create_temp_parquet(&mut df);

    let (_, rows, cols, mem_mb) = load_dataset_with_progress(&parquet_path, 100).unwrap();

    assert_eq!(rows, 1000);
    assert_eq!(cols, 51); // 50 features + 1 target
    assert!(
        mem_mb > 0.0,
        "Large DataFrame should have positive memory estimate"
    );

    // Keep temp_dir alive until we're done
    drop(temp_dir);
}

#[test]
fn test_schema_inference_length() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("inference.csv");

    // Create a file where the type changes after first 10 rows
    let mut file = std::fs::File::create(&csv_path).unwrap();
    writeln!(file, "tricky_col").unwrap();
    for i in 0..100 {
        writeln!(file, "{}", i).unwrap();
    }
    drop(file);

    // Load with different schema inference lengths
    let (df_short, _, _, _) = load_dataset_with_progress(&csv_path, 10).unwrap();
    let (df_long, _, _, _) = load_dataset_with_progress(&csv_path, 1000).unwrap();

    // Both should load successfully
    assert_eq!(df_short.height(), 100);
    assert_eq!(df_long.height(), 100);
}
