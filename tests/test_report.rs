//! Integration tests for report export functions (T-C3) and Parquet-to-CSV
//! conversion (T-C5).

mod common;

use lophi::cli::convert::run_convert;
use lophi::pipeline::{BinningStrategy, IvAnalysis};
use lophi::report::{
    export_gini_analysis_enhanced, export_reduction_report, export_reduction_report_csv,
    package_reduction_reports, ExportParams, ReductionReportBuilder, ReportBuilderParams,
};
use polars::prelude::*;
use tempfile::TempDir;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Build a minimal `ReductionReportBuilder` with one feature that passes all
/// stages and one that is dropped at the missing stage.
fn build_minimal_report() -> lophi::report::ReductionReport {
    let mut builder = ReductionReportBuilder::new(ReportBuilderParams {
        input_file: "input.csv".to_string(),
        output_file: "output.csv".to_string(),
        target_column: "target".to_string(),
        weight_column: None,
        binning_strategy: "quantile".to_string(),
        num_bins: 10,
        missing_threshold: 0.30,
        gini_threshold: 0.05,
        correlation_threshold: 0.40,
    });

    // Simulate missing analysis stage
    let ratios = vec![
        ("feature_good".to_string(), 0.0),
        ("feature_missing".to_string(), 0.80),
    ];
    let missing_drops = vec!["feature_missing".to_string()];
    builder.set_missing_results(&ratios, &missing_drops);

    // Simulate Gini stage (only feature_good survived)
    let gini_analyses = vec![IvAnalysis {
        feature_name: "feature_good".to_string(),
        feature_type: lophi::pipeline::FeatureType::Numeric,
        bins: vec![],
        categories: vec![],
        missing_bin: None,
        iv: 0.5,
        gini: 0.30,
    }];
    builder.set_gini_results(&gini_analyses, &[]);

    // Simulate correlation stage (no drops)
    builder.set_correlation_results(&[], &[]);

    builder.build()
}

fn build_minimal_gini_analyses() -> Vec<IvAnalysis> {
    vec![
        IvAnalysis {
            feature_name: "good_feature".to_string(),
            feature_type: lophi::pipeline::FeatureType::Numeric,
            bins: vec![],
            categories: vec![],
            missing_bin: None,
            iv: 0.5,
            gini: 0.30,
        },
        IvAnalysis {
            feature_name: "weak_feature".to_string(),
            feature_type: lophi::pipeline::FeatureType::Numeric,
            bins: vec![],
            categories: vec![],
            missing_bin: None,
            iv: 0.01,
            gini: 0.02,
        },
    ]
}

// ── T-C3: export_reduction_report ───────────────────────────────────────────

#[test]
fn test_export_reduction_report_creates_valid_json() {
    let report = build_minimal_report();
    let temp_dir = TempDir::new().unwrap();
    let json_path = temp_dir.path().join("report.json");

    export_reduction_report(&report, &json_path).unwrap();

    assert!(json_path.exists(), "JSON report file should be created");

    let contents = std::fs::read_to_string(&json_path).unwrap();
    assert!(!contents.is_empty(), "JSON file should not be empty");

    // Must be valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&contents).expect("Must be valid JSON");

    // Must have key top-level fields
    assert!(parsed.get("metadata").is_some(), "Missing 'metadata' field");
    assert!(parsed.get("summary").is_some(), "Missing 'summary' field");
    assert!(parsed.get("features").is_some(), "Missing 'features' field");
}

#[test]
fn test_export_reduction_report_json_reflects_drop_counts() {
    let report = build_minimal_report();
    let temp_dir = TempDir::new().unwrap();
    let json_path = temp_dir.path().join("report.json");

    export_reduction_report(&report, &json_path).unwrap();

    let contents = std::fs::read_to_string(&json_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();

    let summary = &parsed["summary"];
    assert_eq!(
        summary["dropped_count"], 1,
        "Should report 1 dropped feature"
    );
    assert_eq!(
        summary["initial_features"], 2,
        "Should report 2 initial features"
    );
}

// ── T-C3: export_reduction_report_csv ───────────────────────────────────────

#[test]
fn test_export_reduction_report_csv_creates_file() {
    let report = build_minimal_report();
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("report.csv");

    export_reduction_report_csv(&report, &csv_path).unwrap();

    assert!(csv_path.exists(), "CSV report file should be created");

    let contents = std::fs::read_to_string(&csv_path).unwrap();
    assert!(!contents.is_empty(), "CSV file should not be empty");
}

#[test]
fn test_export_reduction_report_csv_has_correct_header() {
    let report = build_minimal_report();
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("report.csv");

    export_reduction_report_csv(&report, &csv_path).unwrap();

    let contents = std::fs::read_to_string(&csv_path).unwrap();
    let first_line = contents.lines().next().unwrap();

    // Must contain core columns
    assert!(
        first_line.contains("feature"),
        "CSV header should contain 'feature'"
    );
    assert!(
        first_line.contains("status"),
        "CSV header should contain 'status'"
    );
    assert!(
        first_line.contains("gini"),
        "CSV header should contain 'gini'"
    );
}

#[test]
fn test_export_reduction_report_csv_has_expected_row_count() {
    let report = build_minimal_report();
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("report.csv");

    export_reduction_report_csv(&report, &csv_path).unwrap();

    let contents = std::fs::read_to_string(&csv_path).unwrap();
    // 1 header + 2 feature rows
    let line_count = contents.lines().count();
    assert_eq!(
        line_count, 3,
        "CSV should have 1 header + 2 feature rows, got {}",
        line_count
    );
}

// ── T-C3: package_reduction_reports ─────────────────────────────────────────

#[test]
fn test_package_reduction_reports_creates_zip() {
    let temp_dir = TempDir::new().unwrap();

    // Create the three input files
    let gini_path = temp_dir.path().join("gini.json");
    let report_path = temp_dir.path().join("report.json");
    let csv_path = temp_dir.path().join("report.csv");
    let zip_path = temp_dir.path().join("output.zip");

    std::fs::write(&gini_path, r#"{"features":[]}"#).unwrap();
    std::fs::write(&report_path, r#"{"metadata":{}}"#).unwrap();
    std::fs::write(&csv_path, "feature,status\na,kept\n").unwrap();

    package_reduction_reports(&gini_path, &report_path, &csv_path, &zip_path).unwrap();

    assert!(zip_path.exists(), "Zip file should be created");
    assert!(
        zip_path.metadata().unwrap().len() > 0,
        "Zip file should not be empty"
    );
}

#[test]
fn test_package_reduction_reports_zip_contains_expected_files() {
    let temp_dir = TempDir::new().unwrap();

    let gini_path = temp_dir.path().join("gini_analysis.json");
    let report_path = temp_dir.path().join("reduction_report.json");
    let csv_path = temp_dir.path().join("reduction_report.csv");
    let zip_path = temp_dir.path().join("output.zip");

    std::fs::write(&gini_path, r#"{"features":[]}"#).unwrap();
    std::fs::write(&report_path, r#"{"metadata":{}}"#).unwrap();
    std::fs::write(&csv_path, "feature,status\na,kept\n").unwrap();

    package_reduction_reports(&gini_path, &report_path, &csv_path, &zip_path).unwrap();

    // Verify zip file was created and has non-trivial size
    assert!(zip_path.exists(), "Zip file should exist");
    let zip_meta = std::fs::metadata(&zip_path).unwrap();
    assert!(
        zip_meta.len() > 22, // minimum zip file size (empty archive = 22 bytes)
        "Zip file should contain data, got {} bytes",
        zip_meta.len()
    );
}

#[test]
fn test_package_reduction_reports_removes_source_files() {
    // After packaging, the three source files should be deleted.
    let temp_dir = TempDir::new().unwrap();

    let gini_path = temp_dir.path().join("gini.json");
    let report_path = temp_dir.path().join("report.json");
    let csv_path = temp_dir.path().join("report.csv");
    let zip_path = temp_dir.path().join("output.zip");

    std::fs::write(&gini_path, "{}").unwrap();
    std::fs::write(&report_path, "{}").unwrap();
    std::fs::write(&csv_path, "a,b\n").unwrap();

    package_reduction_reports(&gini_path, &report_path, &csv_path, &zip_path).unwrap();

    assert!(
        !gini_path.exists(),
        "Gini JSON should be removed after packaging"
    );
    assert!(
        !report_path.exists(),
        "Report JSON should be removed after packaging"
    );
    assert!(
        !csv_path.exists(),
        "CSV report should be removed after packaging"
    );
}

// ── T-C3: export_gini_analysis_enhanced ─────────────────────────────────────

#[test]
fn test_export_gini_analysis_enhanced_creates_valid_json() {
    let analyses = build_minimal_gini_analyses();
    let dropped = vec!["weak_feature".to_string()];
    let temp_dir = TempDir::new().unwrap();
    let json_path = temp_dir.path().join("gini.json");

    let params = ExportParams {
        input_file: "input.csv",
        target_column: "target",
        weight_column: None,
        binning_strategy: BinningStrategy::Quantile,
        num_bins: 10,
        gini_threshold: 0.05,
        min_category_samples: 5,
        cart_min_bin_pct: None,
    };

    export_gini_analysis_enhanced(&analyses, &dropped, &json_path, &params).unwrap();

    assert!(json_path.exists(), "Gini JSON should be created");

    let contents = std::fs::read_to_string(&json_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents).expect("Must be valid JSON");

    assert!(parsed.get("metadata").is_some());
    assert!(parsed.get("summary").is_some());
    assert!(parsed.get("features").is_some());

    let features = parsed["features"].as_array().unwrap();
    assert_eq!(features.len(), 2, "Should have 2 feature entries");
}

// ── T-C5: Parquet-to-CSV conversion (run_convert with .parquet input) ────────

fn create_test_parquet(temp_dir: &TempDir, name: &str, df: &mut DataFrame) -> std::path::PathBuf {
    let path = temp_dir.path().join(name);
    let file = std::fs::File::create(&path).unwrap();
    ParquetWriter::new(file).finish(df).unwrap();
    path
}

#[test]
fn test_parquet_to_csv_round_trip() {
    let mut df = df! {
        "id"     => [1i32, 2, 3, 4, 5],
        "value"  => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "target" => [0i32, 1, 0, 1, 0],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let parquet_path = create_test_parquet(&temp_dir, "data.parquet", &mut df);
    let csv_path = temp_dir.path().join("data.csv");

    run_convert(&parquet_path, Some(&csv_path), 1000, false).unwrap();

    assert!(csv_path.exists(), "CSV output file should be created");

    let result_df = CsvReadOptions::default()
        .with_infer_schema_length(Some(100))
        .try_into_reader_with_file_path(Some(csv_path))
        .unwrap()
        .finish()
        .unwrap();

    assert_eq!(result_df.height(), 5, "Row count should be preserved");
    assert_eq!(result_df.width(), 3, "Column count should be preserved");
}

#[test]
fn test_parquet_to_csv_preserves_column_names() {
    let mut df = df! {
        "alpha" => [1i32, 2, 3],
        "beta"  => [4.0f64, 5.0, 6.0],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let parquet_path = create_test_parquet(&temp_dir, "cols.parquet", &mut df);
    let csv_path = temp_dir.path().join("cols.csv");

    run_convert(&parquet_path, Some(&csv_path), 1000, false).unwrap();

    let result_df = CsvReadOptions::default()
        .with_infer_schema_length(Some(100))
        .try_into_reader_with_file_path(Some(csv_path))
        .unwrap()
        .finish()
        .unwrap();

    let col_names: Vec<&str> = result_df
        .get_column_names()
        .iter()
        .map(|s| s.as_str())
        .collect();
    assert!(
        col_names.contains(&"alpha"),
        "Should preserve 'alpha' column"
    );
    assert!(col_names.contains(&"beta"), "Should preserve 'beta' column");
}

#[test]
fn test_parquet_to_csv_preserves_null_values() {
    let mut df = df! {
        "feature" => [Some(1.0f64), None, Some(3.0), None, Some(5.0)],
        "target"  => [0i32, 1, 0, 1, 0],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let parquet_path = create_test_parquet(&temp_dir, "nulls.parquet", &mut df);
    let csv_path = temp_dir.path().join("nulls.csv");

    run_convert(&parquet_path, Some(&csv_path), 1000, false).unwrap();

    let result_df = CsvReadOptions::default()
        .with_infer_schema_length(Some(100))
        .try_into_reader_with_file_path(Some(csv_path))
        .unwrap()
        .finish()
        .unwrap();

    let null_count = result_df.column("feature").unwrap().null_count();
    assert_eq!(
        null_count, 2,
        "Null values should be preserved through conversion"
    );
}

#[test]
fn test_parquet_to_csv_auto_output_path() {
    let mut df = df! {
        "a" => [1i32, 2, 3],
    }
    .unwrap();

    let temp_dir = TempDir::new().unwrap();
    let parquet_path = create_test_parquet(&temp_dir, "auto.parquet", &mut df);

    // No explicit output path
    run_convert(&parquet_path, None, 1000, false).unwrap();

    let expected_csv = temp_dir.path().join("auto.csv");
    assert!(
        expected_csv.exists(),
        "Auto-derived CSV output should exist at {:?}",
        expected_csv
    );
}
