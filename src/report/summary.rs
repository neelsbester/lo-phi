//! Reduction summary report generation

use comfy_table::{presets::UTF8_FULL_CONDENSED, Attribute, Cell, Color, Table};
use console::style;

/// Summary of the feature reduction process
#[derive(Debug, Default)]
pub struct ReductionSummary {
    pub initial_features: usize,
    pub final_features: usize,
    pub dropped_missing: Vec<String>,
    pub dropped_correlation: Vec<String>,
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

    pub fn add_correlation_drops(&mut self, features: Vec<String>) {
        self.final_features -= features.len();
        self.dropped_correlation = features;
    }

    pub fn display(&self) {
        println!();
        println!(
            "    {} {}",
            style("📋").cyan(),
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
            Cell::new("📁 Initial Features"),
            Cell::new(self.initial_features),
        ]);

        table.add_row(vec![
            Cell::new("🗑️  Dropped (Missing)"),
            Cell::new(self.dropped_missing.len()).fg(if self.dropped_missing.is_empty() {
                Color::White
            } else {
                Color::Red
            }),
        ]);

        table.add_row(vec![
            Cell::new("🔗 Dropped (Correlation)"),
            Cell::new(self.dropped_correlation.len()).fg(if self.dropped_correlation.is_empty() {
                Color::White
            } else {
                Color::Red
            }),
        ]);

        table.add_row(vec![
            Cell::new("✅ Final Features"),
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
            Cell::new("📉 Reduction"),
            Cell::new(format!("{:.1}%", reduction_pct))
                .fg(color)
                .add_attribute(Attribute::Bold),
        ]);

        // Indent the table
        for line in table.to_string().lines() {
            println!("    {}", line);
        }

        // Show dropped features details if any
        if !self.dropped_missing.is_empty() || !self.dropped_correlation.is_empty() {
            println!();
            println!(
                "    {} {}",
                style("📝").cyan(),
                style("DROPPED FEATURES").white().bold()
            );
            println!("    {}", style("─".repeat(50)).dim());

            if !self.dropped_missing.is_empty() {
                println!();
                println!(
                    "      {} {}:",
                    style("High Missing Values").yellow(),
                    style(format!("({})", self.dropped_missing.len())).dim()
                );
                for feature in &self.dropped_missing {
                    println!("        {} {}", style("•").dim(), feature);
                }
            }

            if !self.dropped_correlation.is_empty() {
                println!();
                println!(
                    "      {} {}:",
                    style("High Correlation").yellow(),
                    style(format!("({})", self.dropped_correlation.len())).dim()
                );
                for feature in &self.dropped_correlation {
                    println!("        {} {}", style("•").dim(), feature);
                }
            }
        }
    }
}
