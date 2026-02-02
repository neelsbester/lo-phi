//! CLI module - argument parsing and interactive prompts

mod args;
mod config_menu;
pub mod convert;
pub mod wizard;

pub use args::{Cli, Commands};
pub use config_menu::{
    run_config_menu, run_file_selector, run_target_mapping_selector, Config, ConfigResult,
    FileSelectResult, TargetMappingResult,
};
#[allow(unused_imports)] // Used in Phase 3/4 when wizard is integrated
pub use wizard::{run_wizard, ConversionConfig, WizardResult};
