//! Weight extraction and validation utilities

use anyhow::{bail, Result};
use polars::prelude::*;

/// Extract weights from a DataFrame column, or return default weights of 1.0.
///
/// # Arguments
/// * `df` - The DataFrame to extract weights from
/// * `weight_column` - Optional name of the weight column
///
/// # Returns
/// * `Ok(Vec<f64>)` - Vector of weights (one per row)
/// * `Err` - If weight column doesn't exist, is non-numeric, or contains negative values
///
/// # Behavior
/// - If `weight_column` is None, returns a vector of 1.0s (equal weights)
/// - If weight column contains null values, they default to 1.0 with a warning
/// - Negative weights cause an error
pub fn get_weights(df: &DataFrame, weight_column: Option<&str>) -> Result<Vec<f64>> {
    match weight_column {
        None => {
            // No weight column specified - use equal weights
            Ok(vec![1.0; df.height()])
        }
        Some(col_name) => {
            // Validate column exists
            let column = df.column(col_name).map_err(|_| {
                anyhow::anyhow!("Weight column '{}' not found in DataFrame", col_name)
            })?;

            // Cast to Float64
            let float_col = column.cast(&DataType::Float64).map_err(|_| {
                anyhow::anyhow!(
                    "Weight column '{}' must be numeric (cannot cast to Float64)",
                    col_name
                )
            })?;

            let ca = float_col.f64().map_err(|_| {
                anyhow::anyhow!("Failed to access weight column '{}' as Float64", col_name)
            })?;

            // Extract weights, handling nulls and validating
            let mut weights = Vec::with_capacity(df.height());
            let mut null_count = 0usize;

            for opt_val in ca.iter() {
                match opt_val {
                    Some(w) => {
                        if w.is_nan() {
                            bail!(
                                "Weight column '{}' contains NaN value. All weights must be valid numbers.",
                                col_name
                            );
                        }
                        if w.is_infinite() {
                            bail!(
                                "Weight column '{}' contains infinite value. All weights must be finite.",
                                col_name
                            );
                        }
                        if w < 0.0 {
                            bail!(
                                "Weight column '{}' contains negative value: {}. All weights must be non-negative.",
                                col_name,
                                w
                            );
                        }
                        weights.push(w);
                    }
                    None => {
                        // Null weights default to 1.0
                        null_count += 1;
                        weights.push(1.0);
                    }
                }
            }

            if null_count > 0 {
                eprintln!(
                    "Warning: Weight column '{}' contains {} null value(s), defaulting to weight 1.0",
                    col_name, null_count
                );
            }

            Ok(weights)
        }
    }
}

/// Calculate the total weight (sum of all weights).
/// Useful for computing weighted statistics.
#[inline]
pub fn total_weight(weights: &[f64]) -> f64 {
    weights.iter().sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_df() -> DataFrame {
        df! {
            "feature" => [1.0, 2.0, 3.0, 4.0, 5.0],
            "weight" => [1.0, 2.0, 0.5, 1.5, 1.0],
            "int_weight" => [1i64, 2, 1, 1, 1],
        }
        .unwrap()
    }

    #[test]
    fn test_no_weight_column_returns_ones() {
        let df = create_test_df();
        let weights = get_weights(&df, None).unwrap();
        assert_eq!(weights.len(), 5);
        assert!(weights.iter().all(|&w| w == 1.0));
    }

    #[test]
    fn test_valid_weight_column() {
        let df = create_test_df();
        let weights = get_weights(&df, Some("weight")).unwrap();
        assert_eq!(weights, vec![1.0, 2.0, 0.5, 1.5, 1.0]);
    }

    #[test]
    fn test_integer_weight_column_casts_to_float() {
        let df = create_test_df();
        let weights = get_weights(&df, Some("int_weight")).unwrap();
        assert_eq!(weights, vec![1.0, 2.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn test_missing_weight_column_errors() {
        let df = create_test_df();
        let result = get_weights(&df, Some("nonexistent"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not found in DataFrame"));
    }

    #[test]
    fn test_negative_weight_errors() {
        let df = df! {
            "feature" => [1.0, 2.0],
            "weight" => [1.0, -0.5],
        }
        .unwrap();
        let result = get_weights(&df, Some("weight"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("negative value"));
    }

    #[test]
    fn test_null_weights_default_to_one() {
        let weight_series = Series::new("weight".into(), &[Some(1.0), None, Some(2.0)]);
        let mut df = df! {
            "feature" => [1.0, 2.0, 3.0],
        }
        .unwrap();
        let _ = df.with_column(weight_series).unwrap();

        let weights = get_weights(&df, Some("weight")).unwrap();
        assert_eq!(weights, vec![1.0, 1.0, 2.0]); // null becomes 1.0
    }

    #[test]
    fn test_zero_weights_allowed() {
        let df = df! {
            "feature" => [1.0, 2.0, 3.0],
            "weight" => [1.0, 0.0, 2.0],
        }
        .unwrap();
        let weights = get_weights(&df, Some("weight")).unwrap();
        assert_eq!(weights, vec![1.0, 0.0, 2.0]);
    }

    #[test]
    fn test_total_weight() {
        let weights = vec![1.0, 2.0, 0.5, 1.5];
        assert!((total_weight(&weights) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_nan_weight_errors() {
        let df = df! {
            "feature" => [1.0, 2.0, 3.0],
            "weight" => [1.0, f64::NAN, 2.0],
        }
        .unwrap();
        let result = get_weights(&df, Some("weight"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NaN"));
    }

    #[test]
    fn test_infinite_weight_errors() {
        let df = df! {
            "feature" => [1.0, 2.0, 3.0],
            "weight" => [1.0, f64::INFINITY, 2.0],
        }
        .unwrap();
        let result = get_weights(&df, Some("weight"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("infinite"));
    }

    #[test]
    fn test_neg_infinite_weight_errors() {
        let df = df! {
            "feature" => [1.0, 2.0, 3.0],
            "weight" => [1.0, f64::NEG_INFINITY, 2.0],
        }
        .unwrap();
        let result = get_weights(&df, Some("weight"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("infinite"));
    }
}
