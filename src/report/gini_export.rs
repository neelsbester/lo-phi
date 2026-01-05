//! Gini analysis export functionality

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::pipeline::IvAnalysis;

/// A single feature's Gini analysis with dropped status
#[derive(Serialize)]
pub struct GiniExportEntry {
    /// The analysis results (flattened into the JSON)
    #[serde(flatten)]
    pub analysis: IvAnalysis,
    /// Whether this feature was dropped due to low Gini
    pub dropped: bool,
}

/// Export Gini analysis results to a JSON file
///
/// # Arguments
/// * `analyses` - All feature analyses from the Gini step
/// * `dropped_features` - List of feature names that were dropped
/// * `output_path` - Path to write the JSON file
///
/// # Returns
/// Result indicating success or failure
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

