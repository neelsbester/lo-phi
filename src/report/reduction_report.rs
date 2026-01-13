//! Comprehensive feature reduction report generation
//!
//! Generates a detailed JSON report documenting all features, their analysis results,
//! and the reasons for dropping or keeping each feature.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;

use crate::pipeline::{CorrelatedPair, FeatureType, IvAnalysis};
use crate::report::ReductionSummary;

/// Drop stage enum for tracking where feature was dropped
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DropStage {
    Missing,
    Gini,
    Correlation,
}

/// Missing analysis result for a feature
#[derive(Debug, Clone, Serialize)]
pub struct MissingAnalysisEntry {
    pub ratio: f64,
    pub threshold: f64,
    pub passed: bool,
}

/// Gini analysis result for a feature
#[derive(Debug, Clone, Serialize)]
pub struct GiniAnalysisEntry {
    pub gini: f64,
    pub iv: f64,
    pub threshold: f64,
    pub passed: bool,
    pub feature_type: String,
}

/// Single correlation entry
#[derive(Debug, Clone, Serialize)]
pub struct CorrelationEntry {
    pub feature: String,
    pub correlation: f64,
}

/// Correlation analysis result for a feature
#[derive(Debug, Clone, Serialize)]
pub struct CorrelationAnalysisEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_correlation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlated_with: Option<String>,
    pub threshold: f64,
    pub passed: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub all_correlations: Vec<CorrelationEntry>,
}

/// Complete analysis for a feature
#[derive(Debug, Clone, Serialize)]
pub struct FeatureAnalysis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<MissingAnalysisEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gini: Option<GiniAnalysisEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation: Option<CorrelationAnalysisEntry>,
}

/// Single feature entry in the report
#[derive(Debug, Clone, Serialize)]
pub struct FeatureReportEntry {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dropped_at_stage: Option<DropStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub analysis: FeatureAnalysis,
}

/// Thresholds used in the analysis
#[derive(Debug, Clone, Serialize)]
pub struct ThresholdsConfig {
    pub missing_ratio: f64,
    pub gini: f64,
    pub correlation: f64,
}

/// Settings used in the analysis
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisSettings {
    pub target_column: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_column: Option<String>,
    pub binning_strategy: String,
    pub num_bins: usize,
}

/// Report metadata
#[derive(Debug, Clone, Serialize)]
pub struct ReportMetadata {
    pub timestamp: String,
    pub lophi_version: String,
    pub input_file: String,
    pub output_file: String,
    pub thresholds: ThresholdsConfig,
    pub settings: AnalysisSettings,
}

/// Stage-level summary
#[derive(Debug, Clone, Serialize)]
pub struct StageSummary {
    pub dropped: usize,
    pub threshold_used: f64,
}

/// By-stage breakdown
#[derive(Debug, Clone, Serialize)]
pub struct ByStage {
    pub missing: StageSummary,
    pub gini: StageSummary,
    pub correlation: StageSummary,
}

/// Timing information in milliseconds
#[derive(Debug, Clone, Default, Serialize)]
pub struct TimingInfo {
    pub load_ms: u64,
    pub missing_ms: u64,
    pub gini_ms: u64,
    pub correlation_ms: u64,
    pub save_ms: u64,
    pub total_ms: u64,
}

/// Report summary
#[derive(Debug, Clone, Serialize)]
pub struct ReportSummary {
    pub initial_features: usize,
    pub final_features: usize,
    pub dropped_count: usize,
    pub by_stage: ByStage,
    pub timing: TimingInfo,
}

/// Complete reduction report
#[derive(Debug, Clone, Serialize)]
pub struct ReductionReport {
    pub metadata: ReportMetadata,
    pub summary: ReportSummary,
    pub features: Vec<FeatureReportEntry>,
}

/// Parameters for creating a ReductionReportBuilder
pub struct ReportBuilderParams {
    pub input_file: String,
    pub output_file: String,
    pub target_column: String,
    pub weight_column: Option<String>,
    pub binning_strategy: String,
    pub num_bins: usize,
    pub missing_threshold: f64,
    pub gini_threshold: f64,
    pub correlation_threshold: f64,
}

/// Builder for constructing the reduction report during pipeline execution
pub struct ReductionReportBuilder {
    // Metadata
    input_file: String,
    output_file: String,
    target_column: String,
    weight_column: Option<String>,
    binning_strategy: String,
    num_bins: usize,

    // Thresholds
    missing_threshold: f64,
    gini_threshold: f64,
    correlation_threshold: f64,

    // Per-feature data collected during pipeline
    missing_ratios: HashMap<String, f64>,
    gini_results: HashMap<String, (f64, f64, FeatureType)>, // (gini, iv, type)
    correlation_pairs: Vec<CorrelatedPair>,

    // Drop tracking
    dropped_missing: HashSet<String>,
    dropped_gini: HashSet<String>,
    dropped_correlation: HashSet<String>,
    dropped_correlation_reasons: HashMap<String, (String, f64)>, // feature -> (correlated_with, coefficient)

    // Timing
    timing: TimingInfo,

    // Feature list (all features seen at missing analysis stage)
    all_features: Vec<String>,
}

impl ReductionReportBuilder {
    /// Create a new report builder with the given parameters
    pub fn new(params: ReportBuilderParams) -> Self {
        Self {
            input_file: params.input_file,
            output_file: params.output_file,
            target_column: params.target_column,
            weight_column: params.weight_column,
            binning_strategy: params.binning_strategy,
            num_bins: params.num_bins,
            missing_threshold: params.missing_threshold,
            gini_threshold: params.gini_threshold,
            correlation_threshold: params.correlation_threshold,
            missing_ratios: HashMap::new(),
            gini_results: HashMap::new(),
            correlation_pairs: Vec::new(),
            dropped_missing: HashSet::new(),
            dropped_gini: HashSet::new(),
            dropped_correlation: HashSet::new(),
            dropped_correlation_reasons: HashMap::new(),
            timing: TimingInfo::default(),
            all_features: Vec::new(),
        }
    }

    /// Record missing analysis results
    pub fn set_missing_results(&mut self, ratios: &[(String, f64)], dropped: &[String]) {
        // Store all features seen at this stage (excluding target)
        self.all_features = ratios
            .iter()
            .filter(|(name, _)| name != &self.target_column)
            .map(|(name, _)| name.clone())
            .collect();

        // Store missing ratios
        for (name, ratio) in ratios {
            if name != &self.target_column {
                self.missing_ratios.insert(name.clone(), *ratio);
            }
        }

        // Store dropped features
        for feature in dropped {
            self.dropped_missing.insert(feature.clone());
        }
    }

    /// Record Gini analysis results
    pub fn set_gini_results(&mut self, analyses: &[IvAnalysis], dropped: &[String]) {
        // Store Gini results for each analyzed feature
        for analysis in analyses {
            self.gini_results.insert(
                analysis.feature_name.clone(),
                (analysis.gini, analysis.iv, analysis.feature_type.clone()),
            );
        }

        // Store dropped features
        for feature in dropped {
            self.dropped_gini.insert(feature.clone());
        }
    }

    /// Record correlation analysis results
    pub fn set_correlation_results(&mut self, pairs: &[CorrelatedPair], dropped: &[String]) {
        // Store all correlation pairs
        self.correlation_pairs = pairs.to_vec();

        // Store dropped features with their reasons
        // For each dropped feature, find the pair that caused it to be dropped
        for feature in dropped {
            self.dropped_correlation.insert(feature.clone());

            // Find the strongest correlation for this feature
            let mut max_corr: Option<(String, f64)> = None;
            for pair in pairs {
                if &pair.feature1 == feature || &pair.feature2 == feature {
                    let other = if &pair.feature1 == feature {
                        &pair.feature2
                    } else {
                        &pair.feature1
                    };
                    let abs_corr = pair.correlation.abs();
                    if max_corr.is_none() || abs_corr > max_corr.as_ref().unwrap().1 {
                        max_corr = Some((other.clone(), abs_corr));
                    }
                }
            }
            if let Some((other, corr)) = max_corr {
                self.dropped_correlation_reasons
                    .insert(feature.clone(), (other, corr));
            }
        }
    }

    /// Set timing information from the ReductionSummary
    pub fn set_timing(&mut self, summary: &ReductionSummary) {
        self.timing = TimingInfo {
            load_ms: summary.load_time.as_millis() as u64,
            missing_ms: summary.missing_time.as_millis() as u64,
            gini_ms: summary.gini_time.as_millis() as u64,
            correlation_ms: summary.correlation_time.as_millis() as u64,
            save_ms: summary.save_time.as_millis() as u64,
            total_ms: summary.total_time().as_millis() as u64,
        };
    }

    /// Build the final report
    pub fn build(self) -> ReductionReport {
        let mut features: Vec<FeatureReportEntry> = Vec::new();

        for feature_name in &self.all_features {
            let entry = self.build_feature_entry(feature_name);
            features.push(entry);
        }

        // Sort features: kept first, then by drop stage, then alphabetically
        features.sort_by(|a, b| {
            match (&a.dropped_at_stage, &b.dropped_at_stage) {
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(stage_a), Some(stage_b)) => {
                    let order_a = match stage_a {
                        DropStage::Missing => 0,
                        DropStage::Gini => 1,
                        DropStage::Correlation => 2,
                    };
                    let order_b = match stage_b {
                        DropStage::Missing => 0,
                        DropStage::Gini => 1,
                        DropStage::Correlation => 2,
                    };
                    order_a.cmp(&order_b).then(a.name.cmp(&b.name))
                }
                (None, None) => a.name.cmp(&b.name),
            }
        });

        let dropped_count = self.dropped_missing.len()
            + self.dropped_gini.len()
            + self.dropped_correlation.len();

        ReductionReport {
            metadata: ReportMetadata {
                timestamp: Utc::now().to_rfc3339(),
                lophi_version: env!("CARGO_PKG_VERSION").to_string(),
                input_file: self.input_file,
                output_file: self.output_file,
                thresholds: ThresholdsConfig {
                    missing_ratio: self.missing_threshold,
                    gini: self.gini_threshold,
                    correlation: self.correlation_threshold,
                },
                settings: AnalysisSettings {
                    target_column: self.target_column,
                    weight_column: self.weight_column,
                    binning_strategy: self.binning_strategy,
                    num_bins: self.num_bins,
                },
            },
            summary: ReportSummary {
                initial_features: self.all_features.len(),
                final_features: self.all_features.len() - dropped_count,
                dropped_count,
                by_stage: ByStage {
                    missing: StageSummary {
                        dropped: self.dropped_missing.len(),
                        threshold_used: self.missing_threshold,
                    },
                    gini: StageSummary {
                        dropped: self.dropped_gini.len(),
                        threshold_used: self.gini_threshold,
                    },
                    correlation: StageSummary {
                        dropped: self.dropped_correlation.len(),
                        threshold_used: self.correlation_threshold,
                    },
                },
                timing: self.timing,
            },
            features,
        }
    }

    /// Build a single feature entry
    fn build_feature_entry(&self, feature_name: &str) -> FeatureReportEntry {
        // Determine status and drop stage
        let (status, dropped_at_stage, reason) = if self.dropped_missing.contains(feature_name) {
            let ratio = self.missing_ratios.get(feature_name).copied().unwrap_or(0.0);
            (
                "dropped".to_string(),
                Some(DropStage::Missing),
                Some(format!(
                    "Missing ratio {:.2} exceeded threshold {:.2}",
                    ratio, self.missing_threshold
                )),
            )
        } else if self.dropped_gini.contains(feature_name) {
            let gini = self
                .gini_results
                .get(feature_name)
                .map(|(g, _, _)| *g)
                .unwrap_or(0.0);
            (
                "dropped".to_string(),
                Some(DropStage::Gini),
                Some(format!(
                    "Gini coefficient {:.4} below threshold {:.4}",
                    gini, self.gini_threshold
                )),
            )
        } else if self.dropped_correlation.contains(feature_name) {
            let reason = if let Some((other, corr)) =
                self.dropped_correlation_reasons.get(feature_name)
            {
                format!(
                    "Correlated with {} (r={:.4}), dropped due to higher correlation frequency",
                    other, corr
                )
            } else {
                "Dropped due to high correlation".to_string()
            };
            (
                "dropped".to_string(),
                Some(DropStage::Correlation),
                Some(reason),
            )
        } else {
            ("kept".to_string(), None, None)
        };

        // Build analysis section
        let missing_analysis = self.missing_ratios.get(feature_name).map(|ratio| {
            let passed = !self.dropped_missing.contains(feature_name);
            MissingAnalysisEntry {
                ratio: *ratio,
                threshold: self.missing_threshold,
                passed,
            }
        });

        // Gini analysis is only available if feature wasn't dropped at missing stage
        let gini_analysis = if !self.dropped_missing.contains(feature_name) {
            self.gini_results
                .get(feature_name)
                .map(|(gini, iv, feature_type)| {
                    let passed = !self.dropped_gini.contains(feature_name);
                    GiniAnalysisEntry {
                        gini: *gini,
                        iv: *iv,
                        threshold: self.gini_threshold,
                        passed,
                        feature_type: format!("{:?}", feature_type),
                    }
                })
        } else {
            None
        };

        // Correlation analysis is only available if feature wasn't dropped at missing or gini stage
        let correlation_analysis = if !self.dropped_missing.contains(feature_name)
            && !self.dropped_gini.contains(feature_name)
        {
            // Find all correlations for this feature that exceed threshold
            let mut correlations: Vec<CorrelationEntry> = self
                .correlation_pairs
                .iter()
                .filter_map(|pair| {
                    if pair.feature1 == feature_name {
                        Some(CorrelationEntry {
                            feature: pair.feature2.clone(),
                            correlation: pair.correlation,
                        })
                    } else if pair.feature2 == feature_name {
                        Some(CorrelationEntry {
                            feature: pair.feature1.clone(),
                            correlation: pair.correlation,
                        })
                    } else {
                        None
                    }
                })
                .collect();

            // Sort by absolute correlation descending
            correlations.sort_by(|a, b| {
                b.correlation
                    .abs()
                    .partial_cmp(&a.correlation.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let max_correlation = correlations.first().map(|c| c.correlation.abs());
            let correlated_with = correlations.first().map(|c| c.feature.clone());
            let passed = !self.dropped_correlation.contains(feature_name);

            Some(CorrelationAnalysisEntry {
                max_correlation,
                correlated_with,
                threshold: self.correlation_threshold,
                passed,
                all_correlations: correlations,
            })
        } else {
            None
        };

        FeatureReportEntry {
            name: feature_name.to_string(),
            status,
            dropped_at_stage,
            reason,
            analysis: FeatureAnalysis {
                missing: missing_analysis,
                gini: gini_analysis,
                correlation: correlation_analysis,
            },
        }
    }
}

/// Export the reduction report to a JSON file
pub fn export_reduction_report(report: &ReductionReport, output_path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(report)
        .context("Failed to serialize reduction report to JSON")?;

    std::fs::write(output_path, json)
        .with_context(|| format!("Failed to write reduction report to {}", output_path.display()))?;

    Ok(())
}

/// Export a CSV summary of the reduction report
///
/// Creates a human-readable CSV with one row per feature showing:
/// - Feature name, status, drop stage, reason
/// - Key metrics: missing ratio, Gini, IV, max correlation
/// - All correlated features (semicolon-separated)
pub fn export_reduction_report_csv(report: &ReductionReport, output_path: &Path) -> Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(output_path)
        .with_context(|| format!("Failed to create CSV file: {}", output_path.display()))?;

    // Write header
    writeln!(
        file,
        "feature,status,dropped_at_stage,reason,missing_ratio,gini,iv,feature_type,max_correlation,correlated_with"
    )?;

    // Write each feature
    for feature in &report.features {
        let stage = feature
            .dropped_at_stage
            .as_ref()
            .map(|s| format!("{:?}", s).to_lowercase())
            .unwrap_or_default();

        let reason = feature
            .reason
            .as_ref()
            .map(|r| escape_csv_field(r))
            .unwrap_or_default();

        let missing_ratio = feature
            .analysis
            .missing
            .as_ref()
            .map(|m| format!("{:.4}", m.ratio))
            .unwrap_or_default();

        let gini = feature
            .analysis
            .gini
            .as_ref()
            .map(|g| format!("{:.4}", g.gini))
            .unwrap_or_default();

        let iv = feature
            .analysis
            .gini
            .as_ref()
            .map(|g| format!("{:.4}", g.iv))
            .unwrap_or_default();

        let feature_type = feature
            .analysis
            .gini
            .as_ref()
            .map(|g| g.feature_type.clone())
            .unwrap_or_default();

        let max_corr = feature
            .analysis
            .correlation
            .as_ref()
            .and_then(|c| c.max_correlation)
            .map(|c| format!("{:.4}", c))
            .unwrap_or_default();

        // Build list of all correlated features with their coefficients
        // Format: "feature_1: 0.92 | feature_2: 0.88" (pipe-separated for clarity)
        let correlated_with = feature
            .analysis
            .correlation
            .as_ref()
            .map(|c| {
                if c.all_correlations.is_empty() {
                    String::new()
                } else {
                    let pairs: Vec<String> = c
                        .all_correlations
                        .iter()
                        .map(|entry| format!("{}: {:.4}", entry.feature, entry.correlation))
                        .collect();
                    // Always quote if there are multiple correlations for cleaner CSV display
                    let joined = pairs.join(" | ");
                    if pairs.len() > 1 || joined.contains(',') {
                        format!("\"{}\"", joined.replace('"', "\"\""))
                    } else {
                        joined
                    }
                }
            })
            .unwrap_or_default();

        writeln!(
            file,
            "{},{},{},{},{},{},{},{},{},{}",
            feature.name,
            feature.status,
            stage,
            reason,
            missing_ratio,
            gini,
            iv,
            feature_type,
            max_corr,
            correlated_with
        )?;
    }

    Ok(())
}

/// Escape a field for CSV (handle commas and quotes)
fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Package reduction report files into a zip archive
///
/// Creates a zip file containing:
/// - gini_analysis.json - Detailed WoE binning analysis
/// - reduction_report.json - Full detailed reduction report
/// - reduction_report.csv - Human-readable summary
pub fn package_reduction_reports(
    gini_analysis_path: &Path,
    reduction_report_path: &Path,
    csv_path: &Path,
    zip_path: &Path,
) -> Result<()> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let zip_file = std::fs::File::create(zip_path)
        .with_context(|| format!("Failed to create zip file: {}", zip_path.display()))?;

    let mut zip = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    // Helper closure to add a file to the zip
    let mut add_file_to_zip = |path: &Path, default_name: &str| -> Result<()> {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(default_name);
        zip.start_file(filename, options)
            .with_context(|| format!("Failed to add {} to zip", filename))?;
        let mut content = Vec::new();
        std::fs::File::open(path)
            .with_context(|| format!("Failed to open file: {}", path.display()))?
            .read_to_end(&mut content)?;
        zip.write_all(&content)?;
        Ok(())
    };

    // Add all three files
    add_file_to_zip(gini_analysis_path, "gini_analysis.json")?;
    add_file_to_zip(reduction_report_path, "reduction_report.json")?;
    add_file_to_zip(csv_path, "reduction_report.csv")?;

    zip.finish().context("Failed to finalize zip file")?;

    // Remove the individual files after packaging
    std::fs::remove_file(gini_analysis_path).ok();
    std::fs::remove_file(reduction_report_path).ok();
    std::fs::remove_file(csv_path).ok();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::FeatureType;

    fn create_test_builder() -> ReductionReportBuilder {
        ReductionReportBuilder::new(ReportBuilderParams {
            input_file: "test_input.csv".to_string(),
            output_file: "test_output.csv".to_string(),
            target_column: "target".to_string(),
            weight_column: None,
            binning_strategy: "quantile".to_string(),
            num_bins: 10,
            missing_threshold: 0.5,
            gini_threshold: 0.1,
            correlation_threshold: 0.85,
        })
    }

    #[test]
    fn test_builder_creation() {
        let builder = create_test_builder();
        assert_eq!(builder.missing_threshold, 0.5);
        assert_eq!(builder.gini_threshold, 0.1);
        assert_eq!(builder.correlation_threshold, 0.85);
    }

    #[test]
    fn test_missing_results() {
        let mut builder = create_test_builder();

        let ratios = vec![
            ("feature_1".to_string(), 0.1),
            ("feature_2".to_string(), 0.6),
            ("target".to_string(), 0.0),
        ];
        let dropped = vec!["feature_2".to_string()];

        builder.set_missing_results(&ratios, &dropped);

        assert_eq!(builder.all_features.len(), 2); // excludes target
        assert!(builder.dropped_missing.contains("feature_2"));
        assert!(!builder.dropped_missing.contains("feature_1"));
    }

    #[test]
    fn test_gini_results() {
        let mut builder = create_test_builder();

        let analyses = vec![IvAnalysis {
            feature_name: "feature_1".to_string(),
            feature_type: FeatureType::Numeric,
            bins: vec![],
            categories: vec![],
            missing_bin: None,
            iv: 0.5,
            gini: 0.3,
        }];
        let dropped: Vec<String> = vec![];

        builder.set_gini_results(&analyses, &dropped);

        assert!(builder.gini_results.contains_key("feature_1"));
        let (gini, iv, _) = builder.gini_results.get("feature_1").unwrap();
        assert_eq!(*gini, 0.3);
        assert_eq!(*iv, 0.5);
    }

    #[test]
    fn test_correlation_results() {
        let mut builder = create_test_builder();

        let pairs = vec![CorrelatedPair {
            feature1: "feature_1".to_string(),
            feature2: "feature_2".to_string(),
            correlation: 0.92,
        }];
        let dropped = vec!["feature_1".to_string()];

        builder.set_correlation_results(&pairs, &dropped);

        assert!(builder.dropped_correlation.contains("feature_1"));
        assert!(builder.dropped_correlation_reasons.contains_key("feature_1"));
    }

    #[test]
    fn test_build_report() {
        let mut builder = create_test_builder();

        // Setup missing results
        let ratios = vec![
            ("feature_1".to_string(), 0.1),
            ("feature_2".to_string(), 0.6),
            ("feature_3".to_string(), 0.05),
        ];
        let dropped_missing = vec!["feature_2".to_string()];
        builder.set_missing_results(&ratios, &dropped_missing);

        // Setup gini results (only for features that passed missing)
        let analyses = vec![
            IvAnalysis {
                feature_name: "feature_1".to_string(),
                feature_type: FeatureType::Numeric,
                bins: vec![],
                categories: vec![],
                missing_bin: None,
                iv: 0.5,
                gini: 0.3,
            },
            IvAnalysis {
                feature_name: "feature_3".to_string(),
                feature_type: FeatureType::Numeric,
                bins: vec![],
                categories: vec![],
                missing_bin: None,
                iv: 0.05,
                gini: 0.05,
            },
        ];
        let dropped_gini = vec!["feature_3".to_string()];
        builder.set_gini_results(&analyses, &dropped_gini);

        // No correlation drops in this test
        builder.set_correlation_results(&[], &[]);

        let report = builder.build();

        assert_eq!(report.summary.initial_features, 3);
        assert_eq!(report.summary.final_features, 1);
        assert_eq!(report.summary.dropped_count, 2);
        assert_eq!(report.summary.by_stage.missing.dropped, 1);
        assert_eq!(report.summary.by_stage.gini.dropped, 1);
        assert_eq!(report.summary.by_stage.correlation.dropped, 0);
        assert_eq!(report.features.len(), 3);
    }

    #[test]
    fn test_feature_entry_kept() {
        let mut builder = create_test_builder();

        let ratios = vec![("feature_1".to_string(), 0.1)];
        builder.set_missing_results(&ratios, &[]);

        let analyses = vec![IvAnalysis {
            feature_name: "feature_1".to_string(),
            feature_type: FeatureType::Numeric,
            bins: vec![],
            categories: vec![],
            missing_bin: None,
            iv: 0.5,
            gini: 0.3,
        }];
        builder.set_gini_results(&analyses, &[]);
        builder.set_correlation_results(&[], &[]);

        let report = builder.build();
        let feature = &report.features[0];

        assert_eq!(feature.status, "kept");
        assert!(feature.dropped_at_stage.is_none());
        assert!(feature.reason.is_none());
        assert!(feature.analysis.missing.is_some());
        assert!(feature.analysis.gini.is_some());
        assert!(feature.analysis.correlation.is_some());
    }

    #[test]
    fn test_feature_entry_dropped_missing() {
        let mut builder = create_test_builder();

        let ratios = vec![("feature_1".to_string(), 0.7)];
        let dropped = vec!["feature_1".to_string()];
        builder.set_missing_results(&ratios, &dropped);

        let report = builder.build();
        let feature = &report.features[0];

        assert_eq!(feature.status, "dropped");
        assert!(matches!(
            feature.dropped_at_stage,
            Some(DropStage::Missing)
        ));
        assert!(feature.reason.as_ref().unwrap().contains("Missing ratio"));
        assert!(feature.analysis.missing.is_some());
        assert!(feature.analysis.gini.is_none()); // Not analyzed
        assert!(feature.analysis.correlation.is_none()); // Not analyzed
    }
}
