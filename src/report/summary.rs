//! Reduction summary report generation

use comfy_table::{Cell, Color, Table};

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
        println!("\n📋 Reduction Summary");
        println!("═══════════════════════════════════════");
        
        let mut table = Table::new();
        table.set_header(vec!["Metric", "Value"]);
        
        table.add_row(vec![
            Cell::new("Initial Features"),
            Cell::new(self.initial_features),
        ]);
        
        table.add_row(vec![
            Cell::new("Dropped (High Missing)"),
            Cell::new(self.dropped_missing.len()).fg(Color::Yellow),
        ]);
        
        table.add_row(vec![
            Cell::new("Dropped (High Correlation)"),
            Cell::new(self.dropped_correlation.len()).fg(Color::Yellow),
        ]);
        
        table.add_row(vec![
            Cell::new("Final Features"),
            Cell::new(self.final_features).fg(Color::Green),
        ]);
        
        let reduction_pct = if self.initial_features > 0 {
            ((self.initial_features - self.final_features) as f64 / self.initial_features as f64) * 100.0
        } else {
            0.0
        };
        
        table.add_row(vec![
            Cell::new("Reduction"),
            Cell::new(format!("{:.1}%", reduction_pct)).fg(Color::Cyan),
        ]);
        
        println!("{table}");
        
        if !self.dropped_missing.is_empty() {
            println!("\n🗑️  Dropped (High Missing): {}", self.dropped_missing.join(", "));
        }
        
        if !self.dropped_correlation.is_empty() {
            println!("\n🔗 Dropped (High Correlation): {}", self.dropped_correlation.join(", "));
        }
    }
}

