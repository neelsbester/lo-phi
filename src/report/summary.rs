//! Reduction summary report generation

use std::time::Duration;

use comfy_table::{presets::UTF8_FULL_CONDENSED, Attribute, Cell, Color, Table};
use console::style;

/// Summary of the feature reduction process
#[derive(Debug, Default)]
pub struct ReductionSummary {
    pub initial_features: usize,
    pub final_features: usize,
    pub dropped_missing: Vec<String>,
    pub dropped_gini: Vec<String>,
    pub dropped_correlation: Vec<String>,
    // Timing information
    pub load_time: Duration,
    pub missing_time: Duration,
    pub gini_time: Duration,
    pub correlation_time: Duration,
    pub save_time: Duration,
}

impl ReductionSummary {
    pub fn new(initial_features: usize) -> Self {
        Self {
            initial_features,
            final_features: initial_features,
            ..Default::default()
        }
    }

    pub fn add_missing_drops(&mut self, features: Vec<String>) {
        self.final_features -= features.len();
        self.dropped_missing = features;
    }

    pub fn add_gini_drops(&mut self, features: Vec<String>) {
        self.final_features -= features.len();
        self.dropped_gini = features;
    }

    pub fn add_correlation_drops(&mut self, features: Vec<String>) {
        self.final_features -= features.len();
        self.dropped_correlation = features;
    }

    pub fn set_load_time(&mut self, duration: Duration) {
        self.load_time = duration;
    }

    pub fn set_missing_time(&mut self, duration: Duration) {
        self.missing_time = duration;
    }

    pub fn set_gini_time(&mut self, duration: Duration) {
        self.gini_time = duration;
    }

    pub fn set_correlation_time(&mut self, duration: Duration) {
        self.correlation_time = duration;
    }

    pub fn set_save_time(&mut self, duration: Duration) {
        self.save_time = duration;
    }

    fn format_duration(duration: Duration) -> String {
        let millis = duration.as_millis();
        if millis < 1000 {
            format!("{}ms", millis)
        } else {
            format!("{:.2}s", duration.as_secs_f64())
        }
    }

    pub fn total_time(&self) -> Duration {
        self.load_time + self.missing_time + self.gini_time + self.correlation_time + self.save_time
    }

    pub fn display(&self) {
        println!();
        println!(
            "    {} {}",
            style("✦").cyan(),
            style("REDUCTION SUMMARY").white().bold()
        );
        println!("    {}", style("─".repeat(50)).dim());
        println!();

        let mut table = Table::new();
        table.load_preset(UTF8_FULL_CONDENSED);
        table.set_header(vec![
            Cell::new("Metric").add_attribute(Attribute::Bold),
            Cell::new("Value").add_attribute(Attribute::Bold),
        ]);

        table.add_row(vec![
            Cell::new("❮ Initial Features"),
            Cell::new(self.initial_features),
        ]);

        table.add_row(vec![
            Cell::new("✗ Dropped (Missing)"),
            Cell::new(self.dropped_missing.len()).fg(if self.dropped_missing.is_empty() {
                Color::White
            } else {
                Color::Red
            }),
        ]);

        table.add_row(vec![
            Cell::new("◈ Dropped (Low Gini)"),
            Cell::new(self.dropped_gini.len()).fg(if self.dropped_gini.is_empty() {
                Color::White
            } else {
                Color::Red
            }),
        ]);

        table.add_row(vec![
            Cell::new("⋈ Dropped (Correlation)"),
            Cell::new(self.dropped_correlation.len()).fg(if self.dropped_correlation.is_empty() {
                Color::White
            } else {
                Color::Red
            }),
        ]);

        table.add_row(vec![
            Cell::new("✓ Final Features"),
            Cell::new(self.final_features)
                .fg(Color::Green)
                .add_attribute(Attribute::Bold),
        ]);

        let reduction_pct = if self.initial_features > 0 {
            ((self.initial_features - self.final_features) as f64 / self.initial_features as f64)
                * 100.0
        } else {
            0.0
        };

        let color = if reduction_pct > 30.0 {
            Color::Green
        } else if reduction_pct > 10.0 {
            Color::Yellow
        } else {
            Color::Cyan
        };

        table.add_row(vec![
            Cell::new("↓ Reduction"),
            Cell::new(format!("{:.1}%", reduction_pct))
                .fg(color)
                .add_attribute(Attribute::Bold),
        ]);

        // Indent the table
        for line in table.to_string().lines() {
            println!("    {}", line);
        }

        // Show timing summary
        println!();
        println!(
            "    {} {}",
            style("◇").cyan(),
            style("TIMING").white().bold()
        );
        println!("    {}", style("─".repeat(50)).dim());

        let mut timing_table = Table::new();
        timing_table.load_preset(UTF8_FULL_CONDENSED);
        timing_table.set_header(vec![
            Cell::new("Step").add_attribute(Attribute::Bold),
            Cell::new("Duration").add_attribute(Attribute::Bold),
        ]);

        timing_table.add_row(vec![
            Cell::new("❮ Load Dataset"),
            Cell::new(Self::format_duration(self.load_time)).fg(Color::Cyan),
        ]);
        timing_table.add_row(vec![
            Cell::new("◈ Missing Analysis"),
            Cell::new(Self::format_duration(self.missing_time)).fg(Color::Cyan),
        ]);
        timing_table.add_row(vec![
            Cell::new("⌘ Gini Analysis"),
            Cell::new(Self::format_duration(self.gini_time)).fg(Color::Cyan),
        ]);
        timing_table.add_row(vec![
            Cell::new("⋈ Correlation Analysis"),
            Cell::new(Self::format_duration(self.correlation_time)).fg(Color::Cyan),
        ]);
        timing_table.add_row(vec![
            Cell::new("⊚ Save Results"),
            Cell::new(Self::format_duration(self.save_time)).fg(Color::Cyan),
        ]);
        timing_table.add_row(vec![
            Cell::new("∑ Total Time").add_attribute(Attribute::Bold),
            Cell::new(Self::format_duration(self.total_time()))
                .fg(Color::Green)
                .add_attribute(Attribute::Bold),
        ]);

        for line in timing_table.to_string().lines() {
            println!("    {}", line);
        }
    }
}
