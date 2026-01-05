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

/// Create a progress bar for known-length operations with gradient styling
pub fn create_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.cyan} [{elapsed_precise}] [{bar:50.cyan/blue}] {pos}/{len} ({percent}%)")
            .unwrap()
            // Gradient-like bar characters
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message(message.to_string());
    pb
}

/// Finish a progress bar with a success message
pub fn finish_with_success(pb: &ProgressBar, message: &str) {
    pb.finish_with_message(format!("{} {}", style("✓").green().bold(), message));
}

/// Finish a progress bar with a warning message  
pub fn finish_with_warning(pb: &ProgressBar, message: &str) {
    pb.finish_with_message(format!("{} {}", style("⚠").yellow().bold(), message));
}

/// Finish a progress bar with an error message
pub fn finish_with_error(pb: &ProgressBar, message: &str) {
    pb.finish_with_message(format!("{} {}", style("✗").red().bold(), message));
}
