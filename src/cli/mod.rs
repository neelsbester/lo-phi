//! CLI module - argument parsing and interactive prompts

mod args;
mod config_menu;

pub use args::Args;
pub use config_menu::{run_config_menu, Config, ConfigResult};

