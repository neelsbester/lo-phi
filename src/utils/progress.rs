//! Progress bar helpers using indicatif with modern styling

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

/// Create a spinner for indeterminate progress with Braille animation
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.magenta.bold} {msg}")
            .unwrap()
            // Fancy braille spinner animation
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Finish a progress bar with a success message
pub fn finish_with_success(pb: &ProgressBar, message: &str) {
    pb.finish_with_message(format!("{} {}", style("✓").green().bold(), message));
}
