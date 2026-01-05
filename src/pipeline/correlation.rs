//! Correlation-based feature reduction

use anyhow::Result;
use polars::prelude::*;

/// Represents a correlated pair of features
#[derive(Debug, Clone)]
pub struct CorrelatedPair {
    pub feature1: String,
    pub feature2: String,
    pub correlation: f64,
}

/// Calculate correlations between numeric columns and find highly correlated pairs
pub fn find_correlated_pairs(lf: &LazyFrame, threshold: f64) -> Result<Vec<CorrelatedPair>> {
    let df = lf.clone().collect()?;
    
    // Get numeric columns only
    let numeric_cols: Vec<&str> = df
        .get_columns()
        .iter()
        .filter(|col| col.dtype().is_primitive_numeric())
        .map(|col| col.name().as_str())
        .collect();
    
    let mut correlated_pairs = Vec::new();
    
    // Calculate upper triangle of correlation matrix
    for (i, col1_name) in numeric_cols.iter().enumerate() {
        for col2_name in numeric_cols.iter().skip(i + 1) {
            let col1 = df.column(col1_name)?.cast(&DataType::Float64)?;
            let col2 = df.column(col2_name)?.cast(&DataType::Float64)?;
            
            if let (Ok(s1), Ok(s2)) = (col1.f64(), col2.f64()) {
                if let Some(corr) = pearson_correlation(&s1, &s2) {
                    if corr.abs() > threshold {
                        correlated_pairs.push(CorrelatedPair {
                            feature1: col1_name.to_string(),
                            feature2: col2_name.to_string(),
                            correlation: corr,
                        });
                    }
                }
            }
        }
    }
    
    // Sort by absolute correlation descending
    correlated_pairs.sort_by(|a, b| {
        b.correlation
            .abs()
            .partial_cmp(&a.correlation.abs())
            .unwrap()
    });
    
    Ok(correlated_pairs)
}

/// Calculate Pearson correlation coefficient between two series
fn pearson_correlation(s1: &ChunkedArray<Float64Type>, s2: &ChunkedArray<Float64Type>) -> Option<f64> {
    let n = s1.len();
    if n == 0 || n != s2.len() {
        return None;
    }
    
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_x2 = 0.0;
    let mut sum_y2 = 0.0;
    let mut count = 0.0;
    
    for (x, y) in s1.iter().zip(s2.iter()) {
        if let (Some(x), Some(y)) = (x, y) {
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_x2 += x * x;
            sum_y2 += y * y;
            count += 1.0;
        }
    }
    
    if count == 0.0 {
        return None;
    }
    
    let numerator = count * sum_xy - sum_x * sum_y;
    let denominator = ((count * sum_x2 - sum_x * sum_x) * (count * sum_y2 - sum_y * sum_y)).sqrt();
    
    if denominator == 0.0 {
        None
    } else {
        Some(numerator / denominator)
    }
}

/// Determine which features to drop from correlated pairs
/// Strategy: For each pair, drop the feature that appears more frequently in correlations
pub fn select_features_to_drop(
    pairs: &[CorrelatedPair],
    target_column: &str,
) -> Vec<String> {
    use std::collections::HashMap;
    
    let mut frequency: HashMap<String, usize> = HashMap::new();
    
    for pair in pairs {
        *frequency.entry(pair.feature1.clone()).or_insert(0) += 1;
        *frequency.entry(pair.feature2.clone()).or_insert(0) += 1;
    }
    
    let mut to_drop = Vec::new();
    let mut already_resolved = std::collections::HashSet::new();
    
    for pair in pairs {
        if already_resolved.contains(&pair.feature1) || already_resolved.contains(&pair.feature2) {
            continue;
        }
        
        // Never drop the target column
        if pair.feature1 == target_column {
            to_drop.push(pair.feature2.clone());
            already_resolved.insert(pair.feature2.clone());
        } else if pair.feature2 == target_column {
            to_drop.push(pair.feature1.clone());
            already_resolved.insert(pair.feature1.clone());
        } else {
            // Drop the one with higher frequency (more correlations)
            let freq1 = frequency.get(&pair.feature1).unwrap_or(&0);
            let freq2 = frequency.get(&pair.feature2).unwrap_or(&0);
            
            if freq1 >= freq2 {
                to_drop.push(pair.feature1.clone());
                already_resolved.insert(pair.feature1.clone());
            } else {
                to_drop.push(pair.feature2.clone());
                already_resolved.insert(pair.feature2.clone());
            }
        }
    }
    
    to_drop
}

/// Drop correlated features from the dataset
pub fn drop_correlated_features(lf: LazyFrame, features_to_drop: &[String]) -> LazyFrame {
    if features_to_drop.is_empty() {
        return lf;
    }
    
    let drop_exprs: Vec<Expr> = features_to_drop
        .iter()
        .map(|name| col(name.as_str()))
        .collect();
    
    lf.drop(drop_exprs)
}

