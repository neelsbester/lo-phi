//! Tests for target column mapping functionality

use lophi::pipeline::*;
use polars::prelude::*;

/// Create a DataFrame with string target values ("G" for good, "B" for bad)
fn create_string_target_dataframe() -> DataFrame {
    df! {
        "target" => ["G", "B", "G", "B", "G", "B", "G", "B", "G", "B",
                     "G", "B", "G", "B", "G", "B", "G", "B", "G", "B"],
        "feature1" => [1.0f64, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0,
                       1.1, 2.1, 1.2, 2.2, 1.3, 2.3, 1.4, 2.4, 1.5, 2.5],
        "feature2" => [5.0f64, 4.0, 3.0, 6.0, 5.0, 4.0, 3.0, 6.0, 5.0, 4.0,
                       5.5, 4.5, 3.5, 6.5, 5.2, 4.2, 3.2, 6.2, 5.8, 4.8],
    }.unwrap()
}

/// Create a DataFrame with multi-value target (good, bad, unknown)
fn create_multivalue_target_dataframe() -> DataFrame {
    df! {
        "target" => ["good", "bad", "unknown", "good", "bad", "unknown",
                     "good", "bad", "unknown", "good", "bad", "unknown",
                     "good", "bad", "unknown", "good", "bad", "unknown",
                     "good", "bad"],
        "feature1" => [1.0f64, 8.0, 5.0, 2.0, 9.0, 4.0,
                       1.5, 8.5, 5.5, 2.5, 9.5, 4.5,
                       1.2, 8.2, 5.2, 2.2, 9.2, 4.2,
                       1.8, 8.8],
        "feature2" => [3.0f64, 7.0, 5.0, 4.0, 6.0, 5.0,
                       3.5, 7.5, 5.5, 4.5, 6.5, 5.5,
                       3.2, 7.2, 5.2, 4.2, 6.2, 5.2,
                       3.8, 7.8],
    }.unwrap()
}

/// Create a DataFrame with numeric non-binary target (1, 2, 3)
fn create_numeric_nonbinary_target_dataframe() -> DataFrame {
    df! {
        "target" => [1i32, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2],
        "feature1" => [1.0f64, 5.0, 8.0, 1.5, 5.5, 8.5, 1.2, 5.2, 8.2, 1.8, 5.8, 8.8,
                       1.1, 5.1, 8.1, 1.3, 5.3, 8.3, 1.4, 5.4],
        "feature2" => [2.0f64, 4.0, 6.0, 2.5, 4.5, 6.5, 2.2, 4.2, 6.2, 2.8, 4.8, 6.8,
                       2.1, 4.1, 6.1, 2.3, 4.3, 6.3, 2.4, 4.4],
    }.unwrap()
}

#[test]
fn test_analyze_binary_target_returns_already_binary() {
    let df = df! {
        "target" => [0i32, 1, 0, 1, 0, 1],
        "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0],
    }.unwrap();
    
    let result = analyze_target_column(&df, "target").unwrap();
    assert!(matches!(result, TargetAnalysis::AlreadyBinary));
}

#[test]
fn test_analyze_float_binary_target_returns_already_binary() {
    let df = df! {
        "target" => [0.0f64, 1.0, 0.0, 1.0],
        "feature" => [1.0f64, 2.0, 3.0, 4.0],
    }.unwrap();
    
    let result = analyze_target_column(&df, "target").unwrap();
    assert!(matches!(result, TargetAnalysis::AlreadyBinary));
}

#[test]
fn test_analyze_string_target_needs_mapping() {
    let df = create_string_target_dataframe();
    
    let result = analyze_target_column(&df, "target").unwrap();
    match result {
        TargetAnalysis::NeedsMapping { unique_values } => {
            assert_eq!(unique_values.len(), 2);
            assert!(unique_values.contains(&"G".to_string()));
            assert!(unique_values.contains(&"B".to_string()));
        }
        _ => panic!("Expected NeedsMapping for string target"),
    }
}

#[test]
fn test_analyze_multivalue_target_needs_mapping() {
    let df = create_multivalue_target_dataframe();
    
    let result = analyze_target_column(&df, "target").unwrap();
    match result {
        TargetAnalysis::NeedsMapping { unique_values } => {
            assert_eq!(unique_values.len(), 3);
            assert!(unique_values.contains(&"good".to_string()));
            assert!(unique_values.contains(&"bad".to_string()));
            assert!(unique_values.contains(&"unknown".to_string()));
        }
        _ => panic!("Expected NeedsMapping for multi-value target"),
    }
}

#[test]
fn test_analyze_numeric_nonbinary_target_needs_mapping() {
    let df = create_numeric_nonbinary_target_dataframe();
    
    let result = analyze_target_column(&df, "target").unwrap();
    match result {
        TargetAnalysis::NeedsMapping { unique_values } => {
            assert_eq!(unique_values.len(), 3);
            assert!(unique_values.contains(&"1".to_string()));
            assert!(unique_values.contains(&"2".to_string()));
            assert!(unique_values.contains(&"3".to_string()));
        }
        _ => panic!("Expected NeedsMapping for numeric non-binary target"),
    }
}

#[test]
fn test_create_target_mask_string_values() {
    let df = create_string_target_dataframe();
    let mapping = TargetMapping::new("B".to_string(), "G".to_string());
    
    let mask = create_target_mask(&df, "target", &mapping).unwrap();
    
    // "G" should map to 0, "B" should map to 1
    assert_eq!(mask[0], Some(0)); // G
    assert_eq!(mask[1], Some(1)); // B
    assert_eq!(mask[2], Some(0)); // G
    assert_eq!(mask[3], Some(1)); // B
}

#[test]
fn test_create_target_mask_with_ignored_values() {
    let df = create_multivalue_target_dataframe();
    let mapping = TargetMapping::new("bad".to_string(), "good".to_string());
    
    let mask = create_target_mask(&df, "target", &mapping).unwrap();
    
    // First 3 values: "good", "bad", "unknown"
    assert_eq!(mask[0], Some(0)); // good -> 0
    assert_eq!(mask[1], Some(1)); // bad -> 1
    assert_eq!(mask[2], None);    // unknown -> ignored
}

#[test]
fn test_iv_analysis_with_string_target_mapping() {
    let df = create_string_target_dataframe();
    let mapping = TargetMapping::new("B".to_string(), "G".to_string());
    
    // Should not error with mapping provided
    let result = analyze_features_iv(&df, "target", 5, Some(&mapping));
    assert!(result.is_ok(), "IV analysis should succeed with target mapping");
    
    let analyses = result.unwrap();
    assert_eq!(analyses.len(), 2, "Should analyze 2 features");
    
    // Check that all features have valid Gini values
    for analysis in &analyses {
        assert!(analysis.gini.is_finite(), "Gini should be finite for {}", analysis.feature_name);
    }
}

#[test]
fn test_iv_analysis_with_multivalue_target_ignores_unknown() {
    let df = create_multivalue_target_dataframe();
    let mapping = TargetMapping::new("bad".to_string(), "good".to_string());
    
    // Should analyze only rows with "good" or "bad", ignoring "unknown"
    let result = analyze_features_iv(&df, "target", 5, Some(&mapping));
    assert!(result.is_ok(), "IV analysis should succeed, ignoring unknown values");
    
    let analyses = result.unwrap();
    assert!(!analyses.is_empty(), "Should have analysis results");
}

#[test]
fn test_iv_analysis_without_mapping_on_binary_target() {
    let df = df! {
        "target" => [0i32, 0, 0, 0, 0, 1, 1, 1, 1, 1,
                     0, 0, 0, 0, 0, 1, 1, 1, 1, 1],
        "feature" => [1.0f64, 1.0, 1.0, 2.0, 2.0, 8.0, 9.0, 9.0, 10.0, 10.0,
                      1.5, 1.5, 2.0, 2.5, 3.0, 7.0, 8.0, 8.5, 9.0, 9.5],
    }.unwrap();
    
    // Should work without mapping for binary target
    let result = analyze_features_iv(&df, "target", 5, None);
    assert!(result.is_ok(), "IV analysis should succeed for binary target without mapping");
}

#[test]
fn test_target_mapping_new() {
    let mapping = TargetMapping::new("event".to_string(), "non_event".to_string());
    assert_eq!(mapping.event_value, "event");
    assert_eq!(mapping.non_event_value, "non_event");
}

#[test]
fn test_analyze_empty_target_fails() {
    let df = df! {
        "target" => Vec::<i32>::new(),
        "feature" => Vec::<f64>::new(),
    }.unwrap();
    
    let result = analyze_target_column(&df, "target");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[test]
fn test_analyze_all_null_target_fails() {
    let df = df! {
        "target" => [None::<String>, None, None],
        "feature" => [1.0f64, 2.0, 3.0],
    }.unwrap();
    
    let result = analyze_target_column(&df, "target");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("null"));
}

#[test]
fn test_analyze_nonexistent_target_fails() {
    let df = df! {
        "other_col" => [0i32, 1, 0, 1],
        "feature" => [1.0f64, 2.0, 3.0, 4.0],
    }.unwrap();
    
    let result = analyze_target_column(&df, "target");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

