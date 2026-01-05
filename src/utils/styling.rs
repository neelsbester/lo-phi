//! Terminal styling utilities for a modern, visually appealing TUI

use console::{style, Emoji};
use std::path::Path;

// Decorative dingbat icons with fallbacks
pub static INFO: Emoji<'_, '_> = Emoji("✦ ", "[*] ");
pub static ROCKET: Emoji<'_, '_> = Emoji("❯ ", ">> ");
pub static CHART: Emoji<'_, '_> = Emoji("✧ ", "");
pub static FOLDER: Emoji<'_, '_> = Emoji("❮ ", "");
pub static TARGET: Emoji<'_, '_> = Emoji("⊛ ", "");
pub static SAVE: Emoji<'_, '_> = Emoji("⊚ ", "");
pub static LINK: Emoji<'_, '_> = Emoji("⋈ ", "");

/// Print the application banner with ASCII art
pub fn print_banner(version: &str) {
    let banner = r#"
    ██╗      ██████╗       ██████╗ ██╗  ██╗██╗
    ██║     ██╔═══██╗      ██╔══██╗██║  ██║██║
    ██║     ██║   ██║█████╗██████╔╝███████║██║
    ██║     ██║   ██║╚════╝██╔═══╝ ██╔══██║██║
    ███████╗╚██████╔╝      ██║     ██║  ██║██║
    ╚══════╝ ╚═════╝       ╚═╝     ╚═╝  ╚═╝╚═╝
    "#;

    println!();
    println!("{}", style(banner).cyan().bold());
    println!(
        "    {} {}",
        style("φ").magenta().bold(),
        style("Feature Reduction as simple as phi").dim()
    );
    println!(
        "    {}",
        style(format!("v{}", version)).dim()
    );
    println!("    {}", style("━".repeat(50)).dim());
    println!();
}

/// Print configuration card
pub fn print_config(input: &Path, target: &str, output: &Path, missing_threshold: f64, correlation_threshold: f64) {
    let box_width = 56;
    let line = "─".repeat(box_width - 2);
    
    println!("    ┌{}┐", line);
    println!(
        "    │ {}{}│",
        style("❖ Configuration").cyan().bold(),
        " ".repeat(box_width - 17)
    );
    println!("    ├{}┤", line);
    println!(
        "    │  {} Input:  {:<39}│",
        FOLDER,
        truncate_path(input, 38)
    );
    println!(
        "    │  {} Target: {:<39}│",
        TARGET,
        truncate_string(target, 38)
    );
    println!(
        "    │  {} Output: {:<39}│",
        SAVE,
        truncate_path(output, 38)
    );
    println!("    ├{}┤", line);
    println!(
        "    │  {} Missing threshold:     {:<24}│",
        CHART,
        style(format!("{:.1}%", missing_threshold * 100.0)).yellow()
    );
    println!(
        "    │  {} Correlation threshold: {:<24}│",
        LINK,
        style(format!("{:.2}", correlation_threshold)).yellow()
    );
    println!("    └{}┘", line);
    println!();
}

/// Print a step header with styling
pub fn print_step_header(step_num: u8, title: &str) {
    println!();
    println!(
        "    {} {} {}",
        style(format!("STEP {}", step_num)).cyan().bold(),
        style("│").dim(),
        style(title).white().bold()
    );
    println!("    {}", style("─".repeat(50)).dim());
}

/// Print a success message
pub fn print_success(message: &str) {
    println!("    {} {}", style("✓").green().bold(), style(message).green());
}

/// Print an info message
pub fn print_info(message: &str) {
    println!("    {} {}", INFO, message);
}

/// Print the final completion message
pub fn print_completion() {
    println!();
    println!(
        "    {} {}",
        ROCKET,
        style("Lo-phi reduction complete!").green().bold()
    );
    println!();
}

/// Print a styled count message
pub fn print_count(description: &str, count: usize, threshold_info: Option<&str>) {
    if let Some(info) = threshold_info {
        println!(
            "      Found {} {} {}",
            style(count).yellow().bold(),
            description,
            style(info).dim()
        );
    } else {
        println!(
            "      Found {} {}",
            style(count).yellow().bold(),
            description
        );
    }
}

// Helper functions

fn truncate_path(path: &Path, max_len: usize) -> String {
    let path_str = path.display().to_string();
    truncate_string(&path_str, max_len)
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("...{}", &s[s.len() - max_len + 3..])
    }
}

