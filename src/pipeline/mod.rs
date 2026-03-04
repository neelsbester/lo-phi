//! Pipeline module - orchestrates the reduction steps

pub mod correlation;
pub mod iv;
pub mod loader;
pub mod missing;
pub mod progress;
pub mod sas7bdat;
pub mod solver;
pub mod target;
pub mod weights;

// Re-exports: some items only consumed by tests/benchmarks, not the binary crate
#[allow(unused_imports)]
pub use correlation::{
    compute_cramers_v, compute_eta, find_correlated_pairs, find_correlated_pairs_auto,
    find_correlated_pairs_auto_with_progress, find_correlated_pairs_matrix,
    select_features_to_drop, AssociationMeasure, CorrelatedPair, FeatureMetadata, FeatureToDrop,
};
#[allow(unused_imports)]
pub use iv::{
    analyze_features_iv, analyze_features_iv_with_progress, get_low_gini_features, BinningStrategy,
    CategoricalWoeBin, FeatureType, IvAnalysis, MissingBin, WoeBin,
};
pub use loader::{
    get_column_names, load_dataset_with_progress, load_dataset_with_progress_channel,
};
pub use missing::{analyze_missing_values, get_features_above_threshold};
pub use progress::{create_progress_channel, PipelineStage, ProgressEvent, ProgressSender};
pub use solver::{MonotonicityConstraint, SolverConfig};
#[allow(unused_imports)]
pub use target::{
    analyze_target_column, count_mapped_records, create_target_mask, TargetAnalysis, TargetMapping,
};
pub use weights::get_weights;
