//! Interactive prompts using dialoguer

use anyhow::Result;
use dialoguer::Confirm;

/// Prompt user to confirm proceeding with an action
pub fn confirm_step(message: &str) -> Result<bool> {
    let confirmed = Confirm::new()
        .with_prompt(message)
        .default(true)
        .interact()?;
    Ok(confirmed)
}

/// Prompt user to confirm dropping specific features
pub fn confirm_drop_features(feature_count: usize, step_name: &str) -> Result<bool> {
    let message = format!(
        "Drop {} feature(s) based on {} analysis?",
        feature_count, step_name
    );
    confirm_step(&message)
}

