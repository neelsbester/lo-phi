//! Target column analysis and mapping
//!
//! This module handles detection and mapping of non-binary target columns
//! to the required 0/1 format for IV/Gini analysis.

use anyhow::{Context, Result};
use polars::prelude::*;
use serde::{Deserialize, Serialize};

/// Tolerance for floating point comparison when checking binary 0/1 values
const TOLERANCE: f64 = 1e-9;

/// Mapping configuration for converting target column values to binary 0/1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetMapping {
    /// Value that maps to 1 (event)
    pub event_value: String,
    /// Value that maps to 0 (non-event)
    pub non_event_value: String,
}

impl TargetMapping {
    /// Create a new target mapping
    pub fn new(event_value: String, non_event_value: String) -> Self {
        Self {
            event_value,
            non_event_value,
        }
    }
}

/// Result of analyzing a target column
#[derive(Debug, Clone)]
pub enum TargetAnalysis {
    /// Target column is already binary 0/1, no mapping needed
    AlreadyBinary,
    /// Target column needs mapping - contains these unique values
    NeedsMapping { unique_values: Vec<String> },
}

/// Analyze a target column to determine if it needs value mapping
///
/// # Arguments
/// * `df` - Reference to the DataFrame
/// * `target` - Name of the target column
///
/// # Returns
/// - `AlreadyBinary` if the column contains only 0 and 1 values
/// - `NeedsMapping` with the list of unique values if mapping is required
pub fn analyze_target_column(df: &DataFrame, target: &str) -> Result<TargetAnalysis> {
    let target_col = df
        .column(target)
        .with_context(|| format!("Target column '{}' not found", target))?;

    // Check for empty or all-null column first
    if target_col.len() == 0 {
        anyhow::bail!("Target column '{}' is empty", target);
    }

    if target_col.null_count() == target_col.len() {
        anyhow::bail!("Target column '{}' contains only null values", target);
    }

    // Try to determine if it's already binary 0/1
    // First, check if it's a numeric type that could be binary
    if target_col.dtype().is_primitive_numeric() {
        let float_col = target_col.cast(&DataType::Float64)?;
        let unique = float_col.unique()?;
        let unique_values: Vec<f64> = unique
            .f64()?
            .into_iter()
            .filter_map(|v| v)
            .collect();

        // Check if all values are 0.0 or 1.0
        let is_binary = unique_values.len() <= 2
            && unique_values
                .iter()
                .all(|&v| (v - 0.0).abs() < TOLERANCE || (v - 1.0).abs() < TOLERANCE);

        if is_binary {
            return Ok(TargetAnalysis::AlreadyBinary);
        }
    }

    // Not binary - get unique values as strings for user selection
    let unique_values = get_unique_values_as_strings(target_col)?;

    if unique_values.is_empty() {
        anyhow::bail!("Target column '{}' has no valid (non-null) values", target);
    }

    Ok(TargetAnalysis::NeedsMapping { unique_values })
}

/// Get unique values from a column as strings
fn get_unique_values_as_strings(col: &Column) -> Result<Vec<String>> {
    let unique = col.unique()?;
    
    let values: Vec<String> = match unique.dtype() {
        DataType::String => {
            unique
                .str()?
                .into_iter()
                .filter_map(|v| v.map(|s| s.to_string()))
                .collect()
        }
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
            let cast = unique.cast(&DataType::Int64)?;
            cast.i64()?
                .into_iter()
                .filter_map(|v| v.map(|n| n.to_string()))
                .collect()
        }
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
            let cast = unique.cast(&DataType::UInt64)?;
            cast.u64()?
                .into_iter()
                .filter_map(|v| v.map(|n| n.to_string()))
                .collect()
        }
        DataType::Float32 | DataType::Float64 => {
            let cast = unique.cast(&DataType::Float64)?;
            cast.f64()?
                .into_iter()
                .filter_map(|v| v.map(|n| format!("{}", n)))
                .collect()
        }
        DataType::Boolean => {
            unique
                .bool()?
                .into_iter()
                .filter_map(|v| v.map(|b| b.to_string()))
                .collect()
        }
        _ => {
            // For other types, try to cast to string
            let cast = unique.cast(&DataType::String)?;
            cast.str()?
                .into_iter()
                .filter_map(|v| v.map(|s| s.to_string()))
                .collect()
        }
    };

    // Sort for consistent ordering
    let mut sorted = values;
    sorted.sort();
    Ok(sorted)
}

/// Create a binary target mask based on the mapping
/// 
/// Returns a Vec<Option<i32>> where:
/// - Some(1) for event values
/// - Some(0) for non-event values
/// - None for values that don't match either (to be filtered during analysis)
pub fn create_target_mask(
    df: &DataFrame,
    target: &str,
    mapping: &TargetMapping,
) -> Result<Vec<Option<i32>>> {
    let target_col = df
        .column(target)
        .with_context(|| format!("Target column '{}' not found", target))?;

    let string_values = column_to_string_vec(target_col)?;
    
    let mask: Vec<Option<i32>> = string_values
        .iter()
        .map(|v| match v {
            Some(s) if s == &mapping.event_value => Some(1),
            Some(s) if s == &mapping.non_event_value => Some(0),
            _ => None, // Value doesn't match mapping - will be ignored in analysis
        })
        .collect();

    Ok(mask)
}

/// Convert a column to a Vec of Option<String> for comparison
fn column_to_string_vec(col: &Column) -> Result<Vec<Option<String>>> {
    let values: Vec<Option<String>> = match col.dtype() {
        DataType::String => {
            col.str()?
                .into_iter()
                .map(|v| v.map(|s| s.to_string()))
                .collect()
        }
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
            let cast = col.cast(&DataType::Int64)?;
            cast.i64()?
                .into_iter()
                .map(|v| v.map(|n| n.to_string()))
                .collect()
        }
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
            let cast = col.cast(&DataType::UInt64)?;
            cast.u64()?
                .into_iter()
                .map(|v| v.map(|n| n.to_string()))
                .collect()
        }
        DataType::Float32 | DataType::Float64 => {
            let cast = col.cast(&DataType::Float64)?;
            cast.f64()?
                .into_iter()
                .map(|v| v.map(|n| format!("{}", n)))
                .collect()
        }
        DataType::Boolean => {
            col.bool()?
                .into_iter()
                .map(|v| v.map(|b| b.to_string()))
                .collect()
        }
        _ => {
            // For other types, try to cast to string
            let cast = col.cast(&DataType::String)?;
            cast.str()?
                .into_iter()
                .map(|v| v.map(|s| s.to_string()))
                .collect()
        }
    };

    Ok(values)
}

/// Count how many records match the event and non-event values
pub fn count_mapped_records(
    df: &DataFrame,
    target: &str,
    mapping: &TargetMapping,
) -> Result<(usize, usize, usize)> {
    let mask = create_target_mask(df, target, mapping)?;
    
    let events = mask.iter().filter(|v| **v == Some(1)).count();
    let non_events = mask.iter().filter(|v| **v == Some(0)).count();
    let ignored = mask.iter().filter(|v| v.is_none()).count();
    
    Ok((events, non_events, ignored))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_binary_int_target() {
        let df = df! {
            "target" => [0i32, 1, 0, 1, 0, 1],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0],
        }.unwrap();
        
        let result = analyze_target_column(&df, "target").unwrap();
        assert!(matches!(result, TargetAnalysis::AlreadyBinary));
    }

    #[test]
    fn test_analyze_binary_float_target() {
        let df = df! {
            "target" => [0.0f64, 1.0, 0.0, 1.0],
            "feature" => [1.0f64, 2.0, 3.0, 4.0],
        }.unwrap();
        
        let result = analyze_target_column(&df, "target").unwrap();
        assert!(matches!(result, TargetAnalysis::AlreadyBinary));
    }

    #[test]
    fn test_analyze_string_target() {
        let df = df! {
            "target" => ["G", "B", "G", "B", "G"],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        }.unwrap();
        
        let result = analyze_target_column(&df, "target").unwrap();
        match result {
            TargetAnalysis::NeedsMapping { unique_values } => {
                assert_eq!(unique_values.len(), 2);
                assert!(unique_values.contains(&"G".to_string()));
                assert!(unique_values.contains(&"B".to_string()));
            }
            _ => panic!("Expected NeedsMapping"),
        }
    }

    #[test]
    fn test_analyze_multi_value_target() {
        let df = df! {
            "target" => ["good", "bad", "unknown", "good", "bad"],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        }.unwrap();
        
        let result = analyze_target_column(&df, "target").unwrap();
        match result {
            TargetAnalysis::NeedsMapping { unique_values } => {
                assert_eq!(unique_values.len(), 3);
                assert!(unique_values.contains(&"good".to_string()));
                assert!(unique_values.contains(&"bad".to_string()));
                assert!(unique_values.contains(&"unknown".to_string()));
            }
            _ => panic!("Expected NeedsMapping"),
        }
    }

    #[test]
    fn test_analyze_non_binary_numeric_target() {
        let df = df! {
            "target" => [1i32, 2, 3, 1, 2, 3],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0],
        }.unwrap();
        
        let result = analyze_target_column(&df, "target").unwrap();
        match result {
            TargetAnalysis::NeedsMapping { unique_values } => {
                assert_eq!(unique_values.len(), 3);
            }
            _ => panic!("Expected NeedsMapping"),
        }
    }

    #[test]
    fn test_create_target_mask() {
        let df = df! {
            "target" => ["G", "B", "G", "B", "X"],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        }.unwrap();
        
        let mapping = TargetMapping::new("B".to_string(), "G".to_string());
        let mask = create_target_mask(&df, "target", &mapping).unwrap();
        
        assert_eq!(mask, vec![Some(0), Some(1), Some(0), Some(1), None]);
    }

    #[test]
    fn test_count_mapped_records() {
        let df = df! {
            "target" => ["G", "B", "G", "B", "X", "X"],
            "feature" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0],
        }.unwrap();
        
        let mapping = TargetMapping::new("B".to_string(), "G".to_string());
        let (events, non_events, ignored) = count_mapped_records(&df, "target", &mapping).unwrap();
        
        assert_eq!(events, 2);      // "B" values
        assert_eq!(non_events, 2);  // "G" values
        assert_eq!(ignored, 2);     // "X" values
    }

    #[test]
    fn test_analyze_empty_target() {
        let df = df! {
            "target" => Vec::<i32>::new(),
            "feature" => Vec::<f64>::new(),
        }.unwrap();
        
        let result = analyze_target_column(&df, "target");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_analyze_all_null_target() {
        let df = df! {
            "target" => [None::<String>, None, None],
            "feature" => [1.0f64, 2.0, 3.0],
        }.unwrap();
        
        let result = analyze_target_column(&df, "target");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null"));
    }
}

