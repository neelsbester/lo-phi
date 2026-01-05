//! Pipeline module - orchestrates the reduction steps

pub mod correlation;
pub mod iv;
pub mod loader;
pub mod missing;

pub use correlation::*;
pub use iv::*;
pub use loader::*;
pub use missing::*;

