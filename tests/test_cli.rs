//! Tests for CLI argument parsing

use clap::Parser;
use lophi::cli::Cli;
use std::path::PathBuf;

#[test]
fn test_cli_default_values() {
    let cli = Cli::parse_from(["lophi", "-i", "data.csv", "-t", "target"]);

    assert_eq!(
        cli.missing_threshold, 0.3,
        "Default missing threshold should be 0.3"
    );
    assert_eq!(
        cli.correlation_threshold, 0.95,
        "Default correlation threshold should be 0.95"
    );
    assert_eq!(
        cli.gini_threshold, 0.05,
        "Default gini threshold should be 0.05"
    );
    assert_eq!(cli.gini_bins, 10, "Default gini bins should be 10");
    assert!(!cli.no_confirm, "Default no_confirm should be false");
    assert_eq!(
        cli.infer_schema_length, 10000,
        "Default schema inference should be 10000"
    );
}

#[test]
fn test_cli_custom_thresholds() {
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "--missing-threshold",
        "0.5",
        "--correlation-threshold",
        "0.8",
        "--gini-threshold",
        "0.1",
    ]);

    assert_eq!(cli.missing_threshold, 0.5);
    assert_eq!(cli.correlation_threshold, 0.8);
    assert_eq!(cli.gini_threshold, 0.1);
}

#[test]
fn test_cli_output_path_derivation() {
    let cli = Cli::parse_from(["lophi", "-i", "/path/to/data.csv", "-t", "target"]);

    let output = cli.output_path().unwrap();
    assert_eq!(output, PathBuf::from("/path/to/data_reduced.csv"));
}

#[test]
fn test_cli_output_path_derivation_parquet() {
    let cli = Cli::parse_from(["lophi", "-i", "/path/to/data.parquet", "-t", "target"]);

    let output = cli.output_path().unwrap();
    assert_eq!(output, PathBuf::from("/path/to/data_reduced.parquet"));
}

#[test]
fn test_cli_explicit_output_path() {
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "-o",
        "custom_output.parquet",
    ]);

    let output = cli.output_path().unwrap();
    assert_eq!(output, PathBuf::from("custom_output.parquet"));
}

#[test]
fn test_cli_drop_columns() {
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "--drop-columns",
        "id,timestamp,uuid",
    ]);

    assert_eq!(cli.drop_columns, vec!["id", "timestamp", "uuid"]);
}

#[test]
fn test_cli_single_drop_column() {
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "--drop-columns",
        "id",
    ]);

    assert_eq!(cli.drop_columns, vec!["id"]);
}

#[test]
fn test_cli_no_drop_columns() {
    let cli = Cli::parse_from(["lophi", "-i", "data.csv", "-t", "target"]);

    assert!(cli.drop_columns.is_empty());
}

#[test]
fn test_cli_no_confirm_flag() {
    let cli = Cli::parse_from(["lophi", "-i", "data.csv", "-t", "target", "--no-confirm"]);

    assert!(cli.no_confirm);
}

#[test]
fn test_cli_custom_schema_inference() {
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "--infer-schema-length",
        "5000",
    ]);

    assert_eq!(cli.infer_schema_length, 5000);
}

#[test]
fn test_cli_full_table_scan() {
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "--infer-schema-length",
        "0",
    ]);

    assert_eq!(cli.infer_schema_length, 0);
}

#[test]
fn test_cli_gini_bins() {
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "--gini-bins",
        "20",
    ]);

    assert_eq!(cli.gini_bins, 20);
}

#[test]
fn test_cli_input_method() {
    let cli = Cli::parse_from(["lophi", "-i", "mydata.csv", "-t", "target"]);

    let input = cli.input();
    assert!(input.is_some());
    assert_eq!(input.unwrap(), &PathBuf::from("mydata.csv"));
}

#[test]
fn test_cli_gini_analysis_path() {
    let cli = Cli::parse_from(["lophi", "-i", "/data/myfile.csv", "-t", "target"]);

    let gini_path = cli.gini_analysis_path().unwrap();
    assert_eq!(gini_path, PathBuf::from("/data/myfile_gini_analysis.json"));
}

#[test]
fn test_cli_all_short_flags() {
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "-o",
        "output.parquet",
    ]);

    assert_eq!(cli.input(), Some(&PathBuf::from("data.csv")));
    assert_eq!(cli.target, Some("target".to_string()));
    assert_eq!(cli.output_path().unwrap(), PathBuf::from("output.parquet"));
}

#[test]
fn test_cli_long_flags() {
    let cli = Cli::parse_from([
        "lophi",
        "--input",
        "data.csv",
        "--target",
        "my_target",
        "--output",
        "result.parquet",
    ]);

    assert_eq!(cli.input(), Some(&PathBuf::from("data.csv")));
    assert_eq!(cli.target, Some("my_target".to_string()));
    assert_eq!(cli.output_path().unwrap(), PathBuf::from("result.parquet"));
}

#[test]
fn test_cli_threshold_boundaries() {
    // Test extreme but valid threshold values
    let cli = Cli::parse_from([
        "lophi",
        "-i",
        "data.csv",
        "-t",
        "target",
        "--missing-threshold",
        "0.0",
        "--correlation-threshold",
        "1.0",
        "--gini-threshold",
        "0.0",
    ]);

    assert_eq!(cli.missing_threshold, 0.0);
    assert_eq!(cli.correlation_threshold, 1.0);
    assert_eq!(cli.gini_threshold, 0.0);
}

#[test]
fn test_cli_relative_path() {
    let cli = Cli::parse_from(["lophi", "-i", "./relative/path/data.csv", "-t", "target"]);

    let output = cli.output_path().unwrap();
    assert_eq!(output, PathBuf::from("./relative/path/data_reduced.csv"));
}

#[test]
fn test_cli_no_input_returns_none() {
    // Test when no input is provided (subcommand scenario)
    let cli = Cli::parse_from(["lophi"]);

    assert!(cli.input().is_none());
    assert!(cli.output_path().is_none());
}
