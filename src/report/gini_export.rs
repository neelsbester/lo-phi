//! Gini analysis export functionality

use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;

use crate::pipeline::{BinningStrategy, FeatureType, IvAnalysis};

/// Metadata about the analysis run
#[derive(Serialize)]
pub struct AnalysisMetadata {
    /// Timestamp of the analysis (ISO 8601 format)
    pub timestamp: String,
    /// Lo-phi version
    pub lophi_version: String,
    /// Input file path
    pub input_file: String,
    /// Target column name
    pub target_column: String,
    /// Weight column name (if used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_column: Option<String>,
    /// Binning strategy used
    pub binning_strategy: String,
    /// Number of bins
    pub num_bins: usize,
    /// Gini threshold for dropping features
    pub gini_threshold: f64,
    /// Minimum samples per category
    pub min_category_samples: usize,
}

/// Summary statistics of the analysis
#[derive(Serialize)]
pub struct AnalysisSummary {
    /// Total features analyzed
    pub total_features_analyzed: usize,
    /// Number of numeric features
    pub numeric_features: usize,
    /// Number of categorical features
    pub categorical_features: usize,
    /// Number of features dropped
    pub features_dropped: usize,
    /// Number of features kept
    pub features_kept: usize,
    /// Average IV across all features
    pub avg_iv: f64,
    /// Average Gini across all features
    pub avg_gini: f64,
}

/// A single feature's Gini analysis with dropped status
#[derive(Serialize)]
pub struct GiniExportEntry {
    /// The analysis results (flattened into the JSON)
    #[serde(flatten)]
    pub analysis: IvAnalysis,
    /// Whether this feature was dropped due to low Gini
    pub dropped: bool,
}

/// Complete Gini analysis export with metadata
#[derive(Serialize)]
pub struct GiniAnalysisExport {
    /// Metadata about the analysis run
    pub metadata: AnalysisMetadata,
    /// Summary statistics
    pub summary: AnalysisSummary,
    /// Per-feature analysis results
    pub features: Vec<GiniExportEntry>,
}

/// Parameters for enhanced Gini analysis export
pub struct ExportParams<'a> {
    pub input_file: &'a str,
    pub target_column: &'a str,
    pub weight_column: Option<&'a str>,
    pub binning_strategy: BinningStrategy,
    pub num_bins: usize,
    pub gini_threshold: f64,
    pub min_category_samples: usize,
}

/// Export Gini analysis results to a JSON file with enhanced metadata
///
/// # Arguments
/// * `analyses` - All feature analyses from the Gini step
/// * `dropped_features` - List of feature names that were dropped
/// * `output_path` - Path to write the JSON file
/// * `params` - Export parameters for metadata
///
/// # Returns
/// Result indicating success or failure
pub fn export_gini_analysis_enhanced(
    analyses: &[IvAnalysis],
    dropped_features: &[String],
    output_path: &Path,
    params: &ExportParams,
) -> Result<()> {
    // Build feature entries
    let entries: Vec<GiniExportEntry> = analyses
        .iter()
        .map(|analysis| {
            let dropped = dropped_features.contains(&analysis.feature_name);
            GiniExportEntry {
                analysis: analysis.clone(),
                dropped,
            }
        })
        .collect();

    // Count feature types
    let numeric_features = analyses
        .iter()
        .filter(|a| a.feature_type == FeatureType::Numeric)
        .count();
    let categorical_features = analyses
        .iter()
        .filter(|a| a.feature_type == FeatureType::Categorical)
        .count();

    // Calculate averages
    let avg_iv = if analyses.is_empty() {
        0.0
    } else {
        analyses.iter().map(|a| a.iv).sum::<f64>() / analyses.len() as f64
    };
    let avg_gini = if analyses.is_empty() {
        0.0
    } else {
        analyses.iter().map(|a| a.gini).sum::<f64>() / analyses.len() as f64
    };

    // Build export structure
    let export = GiniAnalysisExport {
        metadata: AnalysisMetadata {
            timestamp: Utc::now().to_rfc3339(),
            lophi_version: env!("CARGO_PKG_VERSION").to_string(),
            input_file: params.input_file.to_string(),
            target_column: params.target_column.to_string(),
            weight_column: params.weight_column.map(|s| s.to_string()),
            binning_strategy: params.binning_strategy.to_string(),
            num_bins: params.num_bins,
            gini_threshold: params.gini_threshold,
            min_category_samples: params.min_category_samples,
        },
        summary: AnalysisSummary {
            total_features_analyzed: analyses.len(),
            numeric_features,
            categorical_features,
            features_dropped: dropped_features.len(),
            features_kept: analyses.len() - dropped_features.len(),
            avg_iv,
            avg_gini,
        },
        features: entries,
    };

    let json = serde_json::to_string_pretty(&export)
        .context("Failed to serialize Gini analysis to JSON")?;

    std::fs::write(output_path, json)
        .with_context(|| format!("Failed to write Gini analysis to {}", output_path.display()))?;

    Ok(())
}

/// Export Gini analysis results to a JSON file (legacy simple format)
///
/// # Arguments
/// * `analyses` - All feature analyses from the Gini step
/// * `dropped_features` - List of feature names that were dropped
/// * `output_path` - Path to write the JSON file
///
/// # Returns
/// Result indicating success or failure
#[allow(dead_code)]
pub fn export_gini_analysis(
    analyses: &[IvAnalysis],
    dropped_features: &[String],
    output_path: &Path,
) -> Result<()> {
    let entries: Vec<GiniExportEntry> = analyses
        .iter()
        .map(|analysis| {
            let dropped = dropped_features.contains(&analysis.feature_name);
            GiniExportEntry {
                analysis: analysis.clone(),
                dropped,
            }
        })
        .collect();

    let json = serde_json::to_string_pretty(&entries)
        .context("Failed to serialize Gini analysis to JSON")?;

    std::fs::write(output_path, json)
        .with_context(|| format!("Failed to write Gini analysis to {}", output_path.display()))?;

    Ok(())
}
