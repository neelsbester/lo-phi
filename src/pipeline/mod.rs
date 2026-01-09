//! Pipeline module - orchestrates the reduction steps

pub mod correlation;
pub mod iv;
pub mod loader;
pub mod missing;
pub mod solver;
pub mod target;
pub mod weights;

pub use correlation::*;
pub use iv::*;
pub use loader::*;
pub use missing::*;
pub use solver::{
    CategoryStats, MonotonicityConstraint, SolverConfig, SolverResult,
    reconstruct_bins_from_solution, solve_categorical_optimal_binning, solve_optimal_binning,
};
pub use target::*;
pub use weights::*;
