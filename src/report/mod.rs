//! Report module - summarizing reduction results

pub mod gini_export;
pub mod reduction_report;
pub mod summary;

// Re-exports: some items only consumed by tests, not the binary crate
#[allow(unused_imports)]
pub use gini_export::{export_gini_analysis, export_gini_analysis_enhanced, ExportParams};
#[allow(unused_imports)]
pub use reduction_report::{
    export_reduction_report, export_reduction_report_csv, package_reduction_reports,
    ByStage, DropStage, FeatureReportEntry, ReductionReport, ReductionReportBuilder,
    ReportBuilderParams, ReportSummary, StageSummary, TimingInfo,
};
pub use summary::ReductionSummary;
