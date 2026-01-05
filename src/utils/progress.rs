//! Progress bar helpers using indicatif

use indicatif::{ProgressBar, ProgressStyle};

/// Create a spinner for indeterminate progress
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}

/// Create a progress bar for known-length operations
pub fn create_progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("█▓▒░"),
    );
    pb.set_message(message.to_string());
    pb
}

/// Finish a progress bar with a success message
pub fn finish_with_success(pb: &ProgressBar, message: &str) {
    pb.finish_with_message(format!("✅ {}", message));
}

/// Finish a progress bar with a warning message
pub fn finish_with_warning(pb: &ProgressBar, message: &str) {
    pb.finish_with_message(format!("⚠️  {}", message));
}

