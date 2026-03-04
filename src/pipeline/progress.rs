//! Progress event channel for in-TUI pipeline progress reporting.
//!
//! Provides a lightweight mpsc-based channel so pipeline stages can send
//! progress events to a TUI overlay without taking a dependency on ratatui.

use std::sync::mpsc;

/// The pipeline stage that a progress event belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStage {
    Loading,
    Validating,
    MissingAnalysis,
    GiniAnalysis,
    CorrelationAnalysis,
    Sampling,
    Converting,
    Saving,
    Reports,
    Complete,
}

/// Lightweight summary counts for the TUI overlay (feature reduction).
#[derive(Debug, Clone, Default)]
pub struct SummaryData {
    pub initial_features: usize,
    pub final_features: usize,
    pub dropped_missing: usize,
    pub dropped_gini: usize,
    pub dropped_correlation: usize,
}

/// Summary data for the sampling TUI overlay.
#[derive(Debug, Clone)]
pub struct SamplingSummaryData {
    pub input_rows: usize,
    pub sampled_rows: usize,
    pub output_path: String,
    pub method: String,
}

/// Summary data for the conversion TUI overlay.
#[derive(Debug, Clone)]
pub struct ConversionSummaryData {
    pub input_format: String,
    pub output_format: String,
    pub row_count: usize,
    pub col_count: usize,
    pub input_size_mb: f64,
    pub output_size_mb: f64,
    pub output_path: String,
}

/// A single progress event emitted by the pipeline.
#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub stage: PipelineStage,
    pub message: String,
    /// Optional detail line, e.g. "142/500 features" or "1.2 MB / 5.0 MB".
    pub detail: Option<String>,
    /// Whether this event marks the *end* of its stage.
    pub is_complete: bool,
    /// Actual elapsed seconds measured by the pipeline thread (for stage_complete events).
    /// When present, the TUI overlay uses this instead of its own wall-clock measurement
    /// to avoid race conditions when both start and complete events are drained in the
    /// same render cycle.
    pub elapsed_secs: Option<f64>,
    /// Reduction summary data, attached only to the `Complete` event.
    pub summary: Option<SummaryData>,
    /// Sampling summary data, attached only to the `Complete` event.
    pub sampling_summary: Option<SamplingSummaryData>,
    /// Conversion summary data, attached only to the `Complete` event.
    pub conversion_summary: Option<ConversionSummaryData>,
}

pub type ProgressSender = mpsc::Sender<ProgressEvent>;
pub type ProgressReceiver = mpsc::Receiver<ProgressEvent>;

/// Create a (sender, receiver) pair for pipeline progress events.
pub fn create_progress_channel() -> (ProgressSender, ProgressReceiver) {
    mpsc::channel()
}

impl ProgressEvent {
    /// Marks the beginning of a new stage.
    pub fn stage_start(stage: PipelineStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
            detail: None,
            is_complete: false,
            elapsed_secs: None,
            summary: None,
            sampling_summary: None,
            conversion_summary: None,
        }
    }

    /// Mid-stage update with an optional detail string (e.g. "142/500 features").
    pub fn update(
        stage: PipelineStage,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            stage,
            message: message.into(),
            detail: Some(detail.into()),
            is_complete: false,
            elapsed_secs: None,
            summary: None,
            sampling_summary: None,
            conversion_summary: None,
        }
    }

    /// Marks the successful completion of a stage with the actual elapsed time.
    pub fn stage_complete(
        stage: PipelineStage,
        message: impl Into<String>,
        elapsed: std::time::Duration,
    ) -> Self {
        Self {
            stage,
            message: message.into(),
            detail: None,
            is_complete: true,
            elapsed_secs: Some(elapsed.as_secs_f64()),
            summary: None,
            sampling_summary: None,
            conversion_summary: None,
        }
    }
}
