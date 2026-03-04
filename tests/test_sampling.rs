//! Integration tests for the sampling module

use lophi::pipeline::{
    analyze_strata, execute_sampling, SampleSize, SamplingConfig, SamplingMethod, StratumSpec,
};
use polars::prelude::{df, CsvReadOptions, DataFrame, LazyFrame, NamedFrom, SerReader, Series};
use std::path::PathBuf;

#[path = "common/mod.rs"]
mod common;

use common::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base_config(method: SamplingMethod) -> SamplingConfig {
    SamplingConfig {
        input: PathBuf::from("test.csv"),
        output: PathBuf::from("test_out.csv"),
        method,
        strata_column: None,
        sample_size: None,
        strata_specs: vec![],
        seed: Some(42),
        infer_schema_length: 10_000,
    }
}

// ---------------------------------------------------------------------------
// Random sampling
// ---------------------------------------------------------------------------

#[test]
fn random_sample_count() {
    let df = create_stratified_test_dataframe(); // 100 rows, 3 cols
    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Count(5));

    let result = execute_sampling(&df, &cfg).unwrap();

    // Shape: 5 rows, 4 columns (3 original + sampling_weight)
    assert_shape(&result, 5, 4);

    // Weight = N / n = 100 / 5 = 20.0
    let weights: Vec<f64> = result
        .column("sampling_weight")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();
    assert!(
        weights.iter().all(|&w| (w - 20.0).abs() < 1e-10),
        "All weights should be 20.0, got: {:?}",
        weights
    );
}

#[test]
fn random_sample_fraction() {
    let df = create_stratified_test_dataframe(); // 100 rows
    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Fraction(0.1)); // 10% of 100 = 10

    let result = execute_sampling(&df, &cfg).unwrap();
    assert_eq!(result.height(), 10, "10% of 100 rows should yield 10 rows");
}

#[test]
fn random_sample_seed_deterministic() {
    let df = create_stratified_test_dataframe();
    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Count(10));
    cfg.seed = Some(42);

    let r1 = execute_sampling(&df, &cfg).unwrap();
    let r2 = execute_sampling(&df, &cfg).unwrap();

    // Compare sampled value columns row-by-row
    let v1: Vec<f64> = r1
        .column("value")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();
    let v2: Vec<f64> = r2
        .column("value")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();

    assert_eq!(v1, v2, "Same seed must produce identical samples");
}

#[test]
fn random_sample_different_seeds() {
    let df = create_stratified_test_dataframe();

    let mut cfg42 = base_config(SamplingMethod::Random);
    cfg42.sample_size = Some(SampleSize::Count(20));
    cfg42.seed = Some(42);

    let mut cfg99 = base_config(SamplingMethod::Random);
    cfg99.sample_size = Some(SampleSize::Count(20));
    cfg99.seed = Some(99);

    let r42 = execute_sampling(&df, &cfg42).unwrap();
    let r99 = execute_sampling(&df, &cfg99).unwrap();

    let v42: Vec<f64> = r42
        .column("value")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();
    let v99: Vec<f64> = r99
        .column("value")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();

    assert_ne!(
        v42, v99,
        "Different seeds should (almost certainly) produce different samples"
    );
}

#[test]
fn random_sample_exceeds_population() {
    let df = create_stratified_test_dataframe(); // 100 rows
    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Count(200));

    let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
    assert!(
        err.contains("exceeds population"),
        "Expected 'exceeds population' in error, got: {err}"
    );
}

#[test]
fn random_sample_zero() {
    let df = create_stratified_test_dataframe();
    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Count(0));

    let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
    assert!(
        err.contains("must be positive"),
        "Expected 'must be positive' in error, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Stratified sampling
// ---------------------------------------------------------------------------

#[test]
fn stratified_basic() {
    let df = create_stratified_test_dataframe();
    // North=30, South=25; sample 5 from North and 3 from South
    let mut cfg = base_config(SamplingMethod::Stratified);
    cfg.strata_column = Some("region".to_string());
    cfg.strata_specs = vec![
        StratumSpec {
            value: "North".to_string(),
            population_count: 30,
            sample_size: 5,
        },
        StratumSpec {
            value: "South".to_string(),
            population_count: 25,
            sample_size: 3,
        },
        StratumSpec {
            value: "East".to_string(),
            population_count: 25,
            sample_size: 2,
        },
    ];

    let result = execute_sampling(&df, &cfg).unwrap();

    assert_eq!(result.height(), 10, "Total rows should be 5+3+2=10");

    // Verify North rows have weight = 30/5 = 6.0
    let regions: Vec<Option<&str>> = result
        .column("region")
        .unwrap()
        .str()
        .unwrap()
        .into_iter()
        .collect();
    let weights: Vec<f64> = result
        .column("sampling_weight")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();

    let north_weights: Vec<f64> = regions
        .iter()
        .zip(weights.iter())
        .filter(|(r, _)| **r == Some("North"))
        .map(|(_, w)| *w)
        .collect();

    assert_eq!(north_weights.len(), 5, "Should have 5 North rows");
    assert!(
        north_weights.iter().all(|&w| (w - 6.0).abs() < 1e-10),
        "North weight should be 30/5=6.0, got: {:?}",
        north_weights
    );
}

#[test]
fn stratified_exceeds_stratum() {
    let df = create_stratified_test_dataframe(); // West=20
    let mut cfg = base_config(SamplingMethod::Stratified);
    cfg.strata_column = Some("region".to_string());
    cfg.strata_specs = vec![StratumSpec {
        value: "West".to_string(),
        population_count: 20,
        sample_size: 50, // exceeds West's 20 rows
    }];

    let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
    assert!(
        err.contains("exceeds population size"),
        "Expected 'exceeds population size' in error, got: {err}"
    );
}

#[test]
fn stratified_null_stratum() {
    // DataFrame with Some/None values in strata column
    let strata_col = Series::new(
        "group".into(),
        [Some("A"), None, Some("A"), None, Some("A")],
    );
    let val_col = Series::new("val".into(), [1.0f64, 2.0, 3.0, 4.0, 5.0]);
    let df = DataFrame::new(vec![strata_col.into(), val_col.into()]).unwrap();

    let mut cfg = base_config(SamplingMethod::Stratified);
    cfg.strata_column = Some("group".to_string());
    cfg.strata_specs = vec![StratumSpec {
        value: "(null)".to_string(),
        population_count: 2,
        sample_size: 1,
    }];

    let result = execute_sampling(&df, &cfg).unwrap();
    assert_eq!(result.height(), 1, "Should sample 1 null-stratum row");
}

#[test]
fn stratified_single_stratum() {
    let df = create_stratified_test_dataframe(); // North=30
    let mut cfg = base_config(SamplingMethod::Stratified);
    cfg.strata_column = Some("region".to_string());
    cfg.strata_specs = vec![StratumSpec {
        value: "North".to_string(),
        population_count: 30,
        sample_size: 10,
    }];

    let result = execute_sampling(&df, &cfg).unwrap();
    assert_eq!(result.height(), 10);
    assert_has_columns(&result, &["region", "value", "category", "sampling_weight"]);
}

// ---------------------------------------------------------------------------
// Equal allocation
// ---------------------------------------------------------------------------

#[test]
fn equal_allocation_basic() {
    let df = create_stratified_test_dataframe(); // 4 strata
    let mut cfg = base_config(SamplingMethod::EqualAllocation);
    cfg.strata_column = Some("region".to_string());
    cfg.sample_size = Some(SampleSize::Count(3)); // 3 per stratum × 4 strata = 12

    let result = execute_sampling(&df, &cfg).unwrap();
    assert_eq!(result.height(), 12, "4 strata × 3 rows each = 12 total");
}

#[test]
fn equal_allocation_exceeds_smallest() {
    let df = create_stratified_test_dataframe(); // West=20 (smallest stratum)
    let mut cfg = base_config(SamplingMethod::EqualAllocation);
    cfg.strata_column = Some("region".to_string());
    cfg.sample_size = Some(SampleSize::Count(25)); // exceeds West's 20

    let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
    assert!(
        err.contains("exceeds population size"),
        "Expected 'exceeds population size' in error, got: {err}"
    );
}

#[test]
fn equal_allocation_weights_vary() {
    let df = create_stratified_test_dataframe();
    let mut cfg = base_config(SamplingMethod::EqualAllocation);
    cfg.strata_column = Some("region".to_string());
    cfg.sample_size = Some(SampleSize::Count(5)); // same n for every stratum

    let result = execute_sampling(&df, &cfg).unwrap();

    // Weights should differ per stratum since N_h varies (30, 25, 25, 20)
    // while n_h is constant (5)
    let weights: Vec<f64> = result
        .column("sampling_weight")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();

    let min_w = weights.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_w = weights.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (max_w - min_w).abs() > 1e-6,
        "Weights should differ across strata (N_h varies); min={min_w}, max={max_w}"
    );
}

// ---------------------------------------------------------------------------
// analyze_strata
// ---------------------------------------------------------------------------

#[test]
fn analyze_strata_counts() {
    let df = create_stratified_test_dataframe();
    let strata = analyze_strata(&df, "region").unwrap();

    let map: std::collections::HashMap<&str, usize> =
        strata.iter().map(|(k, v)| (k.as_str(), *v)).collect();

    assert_eq!(map["North"], 30, "North should have 30 rows");
    assert_eq!(map["South"], 25, "South should have 25 rows");
    assert_eq!(map["East"], 25, "East should have 25 rows");
    assert_eq!(map["West"], 20, "West should have 20 rows");
}

// ---------------------------------------------------------------------------
// Exact weight values
// ---------------------------------------------------------------------------

#[test]
fn sampling_weight_values_exact() {
    // North=30, n=6 → weight=5.0
    let df = create_stratified_test_dataframe();
    let mut cfg = base_config(SamplingMethod::Stratified);
    cfg.strata_column = Some("region".to_string());
    cfg.strata_specs = vec![StratumSpec {
        value: "North".to_string(),
        population_count: 30,
        sample_size: 6,
    }];

    let result = execute_sampling(&df, &cfg).unwrap();
    let weights: Vec<f64> = result
        .column("sampling_weight")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();

    assert_eq!(weights.len(), 6);
    assert!(
        weights.iter().all(|&w| (w - 5.0).abs() < 1e-10),
        "Weight should be 30/6=5.0, got: {:?}",
        weights
    );
}

// ---------------------------------------------------------------------------
// Column preservation
// ---------------------------------------------------------------------------

#[test]
fn all_columns_preserved() {
    let df = create_stratified_test_dataframe();
    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Count(10));

    let result = execute_sampling(&df, &cfg).unwrap();

    assert_has_columns(&result, &["region", "value", "category", "sampling_weight"]);
    assert_eq!(result.width(), 4, "Should have exactly 4 columns");
}

// ---------------------------------------------------------------------------
// Existing weight column guard
// ---------------------------------------------------------------------------

#[test]
fn existing_weight_column_error() {
    let df = df! {
        "region" => ["A", "B", "C"],
        "sampling_weight" => [1.0f64, 2.0, 3.0],
    }
    .unwrap();

    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Count(2));

    let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
    assert!(
        err.contains("sampling_weight"),
        "Expected error mentioning 'sampling_weight', got: {err}"
    );
}

// ---------------------------------------------------------------------------
// CSV round-trip
// ---------------------------------------------------------------------------

#[test]
fn csv_round_trip() {
    let df = create_stratified_test_dataframe();
    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Count(15));

    let mut sampled = execute_sampling(&df, &cfg).unwrap();
    let expected_shape = sampled.shape();

    let (_temp_dir, csv_path) = create_temp_csv(&mut sampled);

    // Read back
    let read_back = CsvReadOptions::default()
        .with_infer_schema_length(Some(100))
        .try_into_reader_with_file_path(Some(csv_path))
        .unwrap()
        .finish()
        .unwrap();

    assert_eq!(
        read_back.shape(),
        expected_shape,
        "CSV round-trip should preserve shape"
    );
}

// ---------------------------------------------------------------------------
// Parquet round-trip
// ---------------------------------------------------------------------------

#[test]
fn parquet_round_trip() {
    let df = create_stratified_test_dataframe();
    let mut cfg = base_config(SamplingMethod::Random);
    cfg.sample_size = Some(SampleSize::Count(15));

    let mut sampled = execute_sampling(&df, &cfg).unwrap();
    let expected_shape = sampled.shape();

    let (_temp_dir, parquet_path) = create_temp_parquet(&mut sampled);

    // Read back
    let read_back = LazyFrame::scan_parquet(&parquet_path, Default::default())
        .unwrap()
        .collect()
        .unwrap();

    assert_eq!(
        read_back.shape(),
        expected_shape,
        "Parquet round-trip should preserve shape"
    );
}
