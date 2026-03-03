//! CLI module - argument parsing and interactive prompts

mod args;
mod config_menu;
pub mod convert;
pub mod progress_overlay;
pub mod shared;
pub mod theme;
pub mod wizard;

pub use args::{Cli, Commands};
pub use config_menu::{
    run_config_menu_keep_tui, run_file_selector, run_target_mapping_selector, Config, ConfigResult,
    FileSelectResult, TargetMappingResult,
};
#[allow(unused_imports)]
pub use wizard::{run_wizard, run_wizard_keep_tui, ConversionConfig, WizardResult};
