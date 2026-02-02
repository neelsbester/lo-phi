//! Interactive TUI Wizard for Lo-phi feature reduction and CSV-to-Parquet conversion
//!
//! This module provides a step-by-step guided wizard interface that walks users through
//! configuring and executing either:
//! - Feature reduction pipeline with comprehensive parameter configuration
//! - CSV to Parquet file conversion
//!
//! The wizard uses a multi-phase approach with dynamic step sequencing based on user choices.
//! It integrates with the existing config_menu module for file selection and reuses the
//! Config type for reduction operations.
//!
//! # Architecture
//!
//! - `WizardState`: Core state machine managing step progression and data accumulation
//! - `WizardStep`: Enum representing each distinct UI screen with embedded state
//! - `WizardData`: Accumulated configuration data across all steps
//! - `WizardResult`: Final output type that branches to reduction or conversion
//!
//! # Flow
//!
//! 1. Task selection (Reduction vs Conversion)
//! 2. Common steps (file selection)
//! 3. Task-specific mandatory steps
//! 4. Optional advanced settings (for reduction only)
//! 5. Summary and confirmation
//! 6. Result generation
//!
//! # Key Features
//!
//! - Dynamic step insertion based on user choices
//! - Per-step validation with inline error messages
//! - Quit confirmation dialog with overlay
//! - Progress bar showing current step position
//! - Context-sensitive help text in footer
//! - Panic-safe terminal cleanup

use std::io::{stdout, Stdout};
use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Terminal,
};

use super::args::Cli;
use super::config_menu::Config;
use crate::pipeline::TargetMapping;

// ============================================================================
// Core Result Types
// ============================================================================

/// Result of wizard execution - branches to either reduction or conversion
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WizardResult {
    /// User completed reduction configuration
    RunReduction(Box<Config>),
    /// User completed conversion configuration
    RunConversion(Box<ConversionConfig>),
    /// User quit the wizard
    Quit,
}

/// Configuration for CSV to Parquet conversion
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ConversionConfig {
    /// Input CSV file path
    pub input: PathBuf,
    /// Output Parquet file path
    pub output: PathBuf,
    /// Number of rows for schema inference (0 = full scan)
    pub infer_schema_length: usize,
    /// Use fast in-memory conversion
    pub fast: bool,
}

// ============================================================================
// Task Selection Types
// ============================================================================

/// Primary task the user wants to accomplish
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum WizardTask {
    /// Feature reduction pipeline
    Reduction,
    /// CSV to Parquet conversion
    Conversion,
}

// ============================================================================
// Step Definitions
// ============================================================================

/// Individual wizard step with embedded UI state
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WizardStep {
    /// Task selection (reduction vs conversion)
    TaskSelection,

    /// File selection (uses external file selector)
    FileSelection,

    /// Target column selection with search/filter
    TargetSelection {
        search: String,
        filtered: Vec<usize>,
        selected: usize,
    },

    /// Target mapping for non-binary targets
    TargetMapping {
        unique_values: Vec<String>,
        event_selected: Option<usize>,
        non_event_selected: Option<usize>,
        focus: TargetMappingFocus,
    },

    /// Missing value threshold configuration
    MissingThreshold {
        input: String,
        error: Option<String>,
    },

    /// Gini threshold configuration
    GiniThreshold {
        input: String,
        error: Option<String>,
    },

    /// Correlation threshold configuration
    CorrelationThreshold {
        input: String,
        error: Option<String>,
    },

    /// Prompt to configure optional settings
    OptionalSettingsPrompt,

    /// Toggle solver usage
    SolverToggle { selected: bool },

    /// Monotonicity constraint selection
    MonotonicitySelection { selected: usize },

    /// Weight column selection with search/filter
    WeightColumn {
        search: String,
        filtered: Vec<usize>,
        selected: usize,
    },

    /// Drop columns multi-select with search/filter
    DropColumns {
        search: String,
        filtered: Vec<usize>,
        selected: usize,
        checked: Vec<bool>,
    },

    /// Schema inference length configuration
    SchemaInference {
        input: String,
        error: Option<String>,
    },

    /// Final summary before execution
    Summary,

    /// Output path for conversion
    OutputPath {
        input: String,
        error: Option<String>,
    },

    /// Conversion mode selection (fast vs streaming)
    ConversionMode { selected: usize },
}

/// Focus state for target mapping step
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum TargetMappingFocus {
    Event,
    NonEvent,
}

impl std::fmt::Display for WizardStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title())
    }
}

impl WizardStep {
    /// Get the display title for this step
    pub fn title(&self) -> &'static str {
        match self {
            WizardStep::TaskSelection => "Task Selection",
            WizardStep::FileSelection => "File Selection",
            WizardStep::TargetSelection { .. } => "Target Column",
            WizardStep::TargetMapping { .. } => "Target Mapping",
            WizardStep::MissingThreshold { .. } => "Missing Threshold",
            WizardStep::GiniThreshold { .. } => "Gini Threshold",
            WizardStep::CorrelationThreshold { .. } => "Correlation Threshold",
            WizardStep::OptionalSettingsPrompt => "Optional Settings",
            WizardStep::SolverToggle { .. } => "Solver Configuration",
            WizardStep::MonotonicitySelection { .. } => "Monotonicity Constraint",
            WizardStep::WeightColumn { .. } => "Weight Column",
            WizardStep::DropColumns { .. } => "Drop Columns",
            WizardStep::SchemaInference { .. } => "Schema Inference",
            WizardStep::Summary => "Summary",
            WizardStep::OutputPath { .. } => "Output Path",
            WizardStep::ConversionMode { .. } => "Conversion Mode",
        }
    }
}

// ============================================================================
// Action Types
// ============================================================================

/// Action to take after handling an event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StepAction {
    /// Move to next step
    NextStep,
    /// Move to previous step
    PrevStep,
    /// User wants to quit
    Quit,
    /// Stay on current step
    Stay,
    /// Complete wizard with result
    Complete(WizardResult),
}

// ============================================================================
// Data Accumulation
// ============================================================================

/// Accumulated data across all wizard steps
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WizardData {
    // Common fields
    pub task: Option<WizardTask>,
    pub input: Option<PathBuf>,

    // Reduction-specific fields
    pub target: Option<String>,
    pub target_mapping: Option<TargetMapping>,
    pub missing_threshold: f64,
    pub gini_threshold: f64,
    pub correlation_threshold: f64,
    pub use_solver: bool,
    pub monotonicity: String,
    pub weight_column: Option<String>,
    pub columns_to_drop: Vec<String>,
    pub infer_schema_length: usize,

    // Conversion-specific fields
    pub conversion_output: Option<PathBuf>,
    pub conversion_fast: bool,

    // Temporary state for multi-step processes
    pub available_columns: Vec<String>,
    pub target_unique_values: Vec<String>,
}

impl Default for WizardData {
    fn default() -> Self {
        Self {
            task: None,
            input: None,
            target: None,
            target_mapping: None,
            missing_threshold: 0.30,
            gini_threshold: 0.05,
            correlation_threshold: 0.40,
            use_solver: true,
            monotonicity: "none".to_string(),
            weight_column: None,
            columns_to_drop: Vec::new(),
            infer_schema_length: 10000,
            conversion_output: None,
            conversion_fast: true,
            available_columns: Vec::new(),
            target_unique_values: Vec::new(),
        }
    }
}

// ============================================================================
// Wizard State Machine
// ============================================================================

/// Main wizard state machine
#[allow(dead_code)]
pub struct WizardState {
    /// Ordered list of steps to execute
    pub steps: Vec<WizardStep>,
    /// Current step index
    pub current_index: usize,
    /// Accumulated data
    pub data: WizardData,
    /// Show quit confirmation dialog
    pub show_quit_confirm: bool,
    /// Selected index for task selection (0 = Reduction, 1 = Conversion)
    pub task_selected_index: usize,
    /// Yes/No selection for optional settings prompt (true = Yes)
    pub optional_yes: bool,
}

impl Default for WizardState {
    fn default() -> Self {
        Self {
            steps: vec![WizardStep::TaskSelection],
            current_index: 0,
            data: WizardData::default(),
            show_quit_confirm: false,
            task_selected_index: 0,
            optional_yes: false,
        }
    }
}

impl WizardState {
    /// Create new wizard state with initial step
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild step sequence based on task selection
    #[allow(dead_code)]
    pub fn build_steps(&mut self) {
        let task = match &self.data.task {
            Some(t) => t,
            None => {
                // No task selected yet, keep just TaskSelection
                self.steps = vec![WizardStep::TaskSelection];
                self.current_index = 0;
                return;
            }
        };

        match task {
            WizardTask::Reduction => {
                // Initialize filtered list with all columns for target selection
                let all_indices: Vec<usize> = (0..self.data.available_columns.len()).collect();

                let mut steps = vec![
                    WizardStep::TaskSelection,
                    WizardStep::FileSelection,
                    WizardStep::TargetSelection {
                        search: String::new(),
                        filtered: all_indices.clone(),
                        selected: 0,
                    },
                    WizardStep::TargetMapping {
                        unique_values: Vec::new(),
                        event_selected: None,
                        non_event_selected: None,
                        focus: TargetMappingFocus::Event,
                    },
                    WizardStep::MissingThreshold {
                        input: format!("{:.2}", self.data.missing_threshold),
                        error: None,
                    },
                    WizardStep::GiniThreshold {
                        input: format!("{:.2}", self.data.gini_threshold),
                        error: None,
                    },
                    WizardStep::CorrelationThreshold {
                        input: format!("{:.2}", self.data.correlation_threshold),
                        error: None,
                    },
                    WizardStep::OptionalSettingsPrompt,
                ];

                // If user said "Yes" to optional settings, insert those steps before Summary
                if self.optional_yes {
                    steps.push(WizardStep::SolverToggle {
                        selected: self.data.use_solver,
                    });
                    steps.push(WizardStep::MonotonicitySelection { selected: 0 });
                    steps.push(WizardStep::WeightColumn {
                        search: String::new(),
                        filtered: all_indices.clone(),
                        selected: 0,
                    });
                    steps.push(WizardStep::DropColumns {
                        search: String::new(),
                        filtered: all_indices.clone(),
                        selected: 0,
                        checked: vec![false; all_indices.len()],
                    });
                    steps.push(WizardStep::SchemaInference {
                        input: self.data.infer_schema_length.to_string(),
                        error: None,
                    });
                }

                steps.push(WizardStep::Summary);
                self.steps = steps;
            }
            WizardTask::Conversion => {
                // Initialize output path with default based on input
                let default_output = self
                    .data
                    .input
                    .as_ref()
                    .map(|p| p.with_extension("parquet").display().to_string())
                    .unwrap_or_default();

                self.steps = vec![
                    WizardStep::TaskSelection,
                    WizardStep::FileSelection,
                    WizardStep::OutputPath {
                        input: default_output,
                        error: None,
                    },
                    WizardStep::ConversionMode { selected: 0 },
                    WizardStep::Summary,
                ];
            }
        }
    }

    /// Move to next step
    pub fn next_step(&mut self) -> Result<()> {
        if self.current_index < self.steps.len() - 1 {
            self.current_index += 1;
        }
        Ok(())
    }

    /// Move to previous step
    pub fn prev_step(&mut self) -> Result<()> {
        if self.current_index > 0 {
            self.current_index -= 1;
        }
        Ok(())
    }

    /// Get current step
    pub fn current_step(&self) -> Option<&WizardStep> {
        self.steps.get(self.current_index)
    }

    /// Get mutable reference to current step
    #[allow(dead_code)]
    pub fn current_step_mut(&mut self) -> Option<&mut WizardStep> {
        self.steps.get_mut(self.current_index)
    }

    /// Check if we're on the last step
    pub fn is_last_step(&self) -> bool {
        self.current_index == self.steps.len() - 1
    }
}

// ============================================================================
// Terminal Setup/Teardown
// ============================================================================

/// Setup terminal for TUI rendering with panic-safe cleanup
pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    // Install panic hook for clean terminal restoration
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        teardown_terminal();
        original_hook(panic_info);
    }));

    let backend = CrosstermBackend::new(stdout());
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore terminal to normal state
pub fn teardown_terminal() {
    let _ = disable_raw_mode();
    let _ = stdout().execute(LeaveAlternateScreen);
}

// ============================================================================
// Entry Point
// ============================================================================

/// Run the wizard interface
#[allow(dead_code)]
pub fn run_wizard(cli: &Cli) -> Result<WizardResult> {
    // Create wizard state and pre-populate from CLI args
    let mut wizard = WizardState::new();

    // Pre-populate data from CLI
    if let Some(input) = &cli.input {
        wizard.data.input = Some(input.clone());
    }
    if let Some(target) = &cli.target {
        wizard.data.target = Some(target.clone());
    }
    if let (Some(event), Some(non_event)) = (&cli.event_value, &cli.non_event_value) {
        wizard.data.target_mapping = Some(TargetMapping::new(event.clone(), non_event.clone()));
    }
    if let Some(weight) = &cli.weight_column {
        wizard.data.weight_column = Some(weight.clone());
    }
    wizard.data.missing_threshold = cli.missing_threshold;
    wizard.data.gini_threshold = cli.gini_threshold;
    wizard.data.correlation_threshold = cli.correlation_threshold;
    wizard.data.use_solver = cli.use_solver;
    wizard.data.monotonicity = cli.monotonicity.clone();
    wizard.data.infer_schema_length = cli.infer_schema_length;
    wizard.data.columns_to_drop = cli.drop_columns.clone();

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Run wizard loop
    let result = run_wizard_loop(&mut terminal, &mut wizard)?;

    // Teardown terminal
    teardown_terminal();

    Ok(result)
}

// ============================================================================
// Event Loop
// ============================================================================

/// Main wizard event loop
fn run_wizard_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    wizard: &mut WizardState,
) -> Result<WizardResult> {
    loop {
        // Draw current state
        terminal.draw(|f| render_wizard(f, wizard))?;

        // Poll for events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Only handle key press events, not release
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Handle quit confirmation overlay first
                if wizard.show_quit_confirm {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            return Ok(WizardResult::Quit);
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            wizard.show_quit_confirm = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                // Show quit confirmation on Q or Esc
                if matches!(
                    key.code,
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                ) {
                    wizard.show_quit_confirm = true;
                    continue;
                }

                // Handle step-specific events
                let action = handle_step_event(wizard, key)?;

                // Process action
                match action {
                    StepAction::NextStep => {
                        wizard.next_step()?;
                    }
                    StepAction::PrevStep => {
                        wizard.prev_step()?;
                    }
                    StepAction::Quit => {
                        wizard.show_quit_confirm = true;
                    }
                    StepAction::Complete(result) => {
                        return Ok(result);
                    }
                    StepAction::Stay => {}
                }
            }
        }
    }
}

/// Handle keyboard event for current step
fn handle_step_event(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    // Common handling for Backspace (go back)
    if key.code == KeyCode::Backspace {
        return if wizard.current_index > 0 {
            Ok(StepAction::PrevStep)
        } else {
            Ok(StepAction::Stay)
        };
    }

    // Dispatch to step-specific handlers
    let step = wizard.current_step().cloned();
    match step {
        Some(WizardStep::TaskSelection) => handle_task_selection(wizard, key),
        Some(WizardStep::FileSelection) => handle_file_selection(wizard, key),
        Some(WizardStep::TargetSelection { .. }) => handle_target_selection(wizard, key),
        Some(WizardStep::TargetMapping { .. }) => handle_target_mapping(wizard, key),
        Some(WizardStep::MissingThreshold { .. }) => handle_missing_threshold(wizard, key),
        Some(WizardStep::GiniThreshold { .. }) => handle_gini_threshold(wizard, key),
        Some(WizardStep::CorrelationThreshold { .. }) => handle_correlation_threshold(wizard, key),
        Some(WizardStep::OptionalSettingsPrompt) => handle_optional_settings_prompt(wizard, key),
        Some(WizardStep::SolverToggle { .. }) => handle_solver_toggle(wizard, key),
        Some(WizardStep::MonotonicitySelection { .. }) => {
            handle_monotonicity_selection(wizard, key)
        }
        Some(WizardStep::WeightColumn { .. }) => handle_weight_column(wizard, key),
        Some(WizardStep::DropColumns { .. }) => handle_drop_columns(wizard, key),
        Some(WizardStep::SchemaInference { .. }) => handle_schema_inference(wizard, key),
        Some(WizardStep::Summary) => handle_summary(wizard, key),
        Some(WizardStep::OutputPath { .. }) => handle_output_path(wizard, key),
        Some(WizardStep::ConversionMode { .. }) => handle_conversion_mode(wizard, key),
        None => Ok(StepAction::Stay),
    }
}

/// Generate final wizard result
fn generate_result(wizard: &WizardState) -> Result<StepAction> {
    match wizard.data.task {
        Some(WizardTask::Reduction) => {
            let input = wizard
                .data
                .input
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No input file selected"))?;
            let target = wizard
                .data
                .target
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No target column selected"))?;

            // Generate output path
            let output = generate_output_path(&input, "_reduced")?;

            let config = Config {
                input: input.clone(),
                target: Some(target),
                output,
                missing_threshold: wizard.data.missing_threshold,
                gini_threshold: wizard.data.gini_threshold,
                correlation_threshold: wizard.data.correlation_threshold,
                columns_to_drop: wizard.data.columns_to_drop.clone(),
                target_mapping: wizard.data.target_mapping.clone(),
                weight_column: wizard.data.weight_column.clone(),
                binning_strategy: "cart".to_string(),
                gini_bins: 10,
                prebins: 20,
                cart_min_bin_pct: 5.0,
                min_category_samples: 5,
                use_solver: wizard.data.use_solver,
                monotonicity: wizard.data.monotonicity.clone(),
                solver_timeout: 30,
                solver_gap: 0.01,
                infer_schema_length: wizard.data.infer_schema_length,
            };

            Ok(StepAction::Complete(WizardResult::RunReduction(Box::new(
                config,
            ))))
        }
        Some(WizardTask::Conversion) => {
            let input = wizard
                .data
                .input
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No input file selected"))?;
            let output = wizard
                .data
                .conversion_output
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No output path specified"))?;

            let config = ConversionConfig {
                input,
                output,
                infer_schema_length: wizard.data.infer_schema_length,
                fast: wizard.data.conversion_fast,
            };

            Ok(StepAction::Complete(WizardResult::RunConversion(Box::new(
                config,
            ))))
        }
        None => Err(anyhow::anyhow!("No task selected")),
    }
}

/// Generate output path with suffix
fn generate_output_path(input: &std::path::Path, suffix: &str) -> Result<PathBuf> {
    let stem = input
        .file_stem()
        .ok_or_else(|| anyhow::anyhow!("Invalid input filename"))?;
    let extension = input
        .extension()
        .ok_or_else(|| anyhow::anyhow!("Input file has no extension"))?;

    let output_name = format!("{}{}", stem.to_string_lossy(), suffix);

    let mut output = input
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    output.push(output_name);
    output.set_extension(extension);

    Ok(output)
}

// ============================================================================
// Rendering
// ============================================================================

/// Render the complete wizard UI
fn render_wizard(f: &mut Frame, wizard: &WizardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Progress bar
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Help bar
        ])
        .split(f.area());

    // Render progress bar
    render_progress_bar(f, chunks[0], wizard);

    // Render current step
    render_step(f, chunks[1], wizard);

    // Render help bar
    render_help_bar(f, chunks[2], wizard);

    // Render quit confirmation overlay if needed
    if wizard.show_quit_confirm {
        render_quit_confirm_overlay(f, wizard);
    }
}

/// Render progress bar showing current step
fn render_progress_bar(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let current = wizard.current_index + 1;
    let total = wizard.steps.len();
    let progress = current as f64 / total as f64;

    let step_title = wizard
        .current_step()
        .map(|s| s.title())
        .unwrap_or("Unknown");

    let label = format!("Step {} of {} - {}", current, total, step_title);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title("Lo-phi Wizard"),
        )
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .label(label)
        .ratio(progress);

    f.render_widget(gauge, area);
}

/// Render the current step
fn render_step(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let step = match wizard.current_step() {
        Some(s) => s,
        None => {
            let text = "Error: No current step";
            let paragraph = Paragraph::new(text)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(paragraph, area);
            return;
        }
    };

    // Dispatch to step-specific renderer (placeholder for now)
    match step {
        WizardStep::TaskSelection => render_task_selection(f, area, wizard),
        WizardStep::FileSelection => render_file_selection(f, area, wizard),
        WizardStep::TargetSelection { .. } => render_target_selection(f, area, wizard),
        WizardStep::TargetMapping { .. } => render_target_mapping(f, area, wizard),
        WizardStep::MissingThreshold { .. } => render_missing_threshold(f, area, wizard),
        WizardStep::GiniThreshold { .. } => render_gini_threshold(f, area, wizard),
        WizardStep::CorrelationThreshold { .. } => render_correlation_threshold(f, area, wizard),
        WizardStep::OptionalSettingsPrompt => render_optional_settings_prompt(f, area, wizard),
        WizardStep::SolverToggle { .. } => render_solver_toggle(f, area, wizard),
        WizardStep::MonotonicitySelection { .. } => render_monotonicity_selection(f, area, wizard),
        WizardStep::WeightColumn { .. } => render_weight_column(f, area, wizard),
        WizardStep::DropColumns { .. } => render_drop_columns(f, area, wizard),
        WizardStep::SchemaInference { .. } => render_schema_inference(f, area, wizard),
        WizardStep::Summary => render_summary(f, area, wizard),
        WizardStep::OutputPath { .. } => render_output_path(f, area, wizard),
        WizardStep::ConversionMode { .. } => render_conversion_mode(f, area, wizard),
    }
}

/// Render help bar with context-appropriate shortcuts
fn render_help_bar(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let help_text = if wizard.is_last_step() {
        "[Enter] Execute | [Backspace] Back | [Q/Esc] Quit"
    } else if wizard.current_index == 0 {
        "[Enter] Continue | [Q/Esc] Quit"
    } else {
        "[Enter] Next | [Backspace] Back | [Q/Esc] Quit"
    };

    let paragraph = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}

/// Render quit confirmation overlay
fn render_quit_confirm_overlay(f: &mut Frame, _wizard: &WizardState) {
    let area = centered_rect(50, 30, f.area());

    // Clear the area
    f.render_widget(Clear, area);

    let block = Block::default()
        .title("Quit Wizard?")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = "Are you sure you want to quit?\n\nAll progress will be lost.\n\n[Y] Yes  [N] No";
    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, inner);
}

/// Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// ============================================================================
// Step Renderers (Placeholders - Phase 3/4 will implement)
// ============================================================================

#[allow(dead_code)]
fn render_task_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let options = [
        (
            "Reduce features",
            "Analyze and reduce dataset features based on missing values, Gini/IV, and correlation",
        ),
        (
            "Convert CSV to Parquet",
            "Convert a CSV file to Parquet format with optional compression",
        ),
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (title, desc))| {
            let style = if i == wizard.task_selected_index {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let content = format!("  {} - {}", title, desc);
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title("What would you like to do?"),
    );

    f.render_widget(list, area);
}

fn handle_task_selection(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    match key.code {
        KeyCode::Up => {
            if wizard.task_selected_index > 0 {
                wizard.task_selected_index -= 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Down => {
            if wizard.task_selected_index < 1 {
                wizard.task_selected_index += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            wizard.data.task = Some(if wizard.task_selected_index == 0 {
                WizardTask::Reduction
            } else {
                WizardTask::Conversion
            });
            wizard.build_steps();
            Ok(StepAction::NextStep)
        }
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_file_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let text = if let Some(input) = &wizard.data.input {
        format!(
            "Input file:\n\n  {}\n\nPress Enter to continue",
            input.display()
        )
    } else {
        "Press Enter to open file selector...".to_string()
    };

    let paragraph = Paragraph::new(text).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title("Select Input File"),
    );

    f.render_widget(paragraph, area);
}

fn handle_file_selection(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    if key.code == KeyCode::Enter {
        if wizard.data.input.is_some() {
            // File already selected (pre-populated from CLI), just advance
            // Load column names
            if let Some(input) = &wizard.data.input {
                wizard.data.available_columns = crate::pipeline::get_column_names(input)?;
            }
            return Ok(StepAction::NextStep);
        }

        // Need to run file selector - this requires special handling in the event loop
        // For now, we'll use a workaround: temporarily exit TUI, run selector, re-enter
        // This needs to be done from the event loop, so we'll mark it with a special action
        // Actually, we can do it right here since we have the wizard state
        teardown_terminal();

        let result = super::config_menu::run_file_selector()?;

        // Re-setup terminal
        // Note: We can't call setup_terminal() here because we're inside the event loop
        // So we'll just enable raw mode and enter alternate screen
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;

        match result {
            super::config_menu::FileSelectResult::Selected(path) => {
                wizard.data.input = Some(path.clone());
                // Load column names
                wizard.data.available_columns = crate::pipeline::get_column_names(&path)?;
                Ok(StepAction::NextStep)
            }
            super::config_menu::FileSelectResult::Cancelled => Ok(StepAction::Stay),
        }
    } else {
        Ok(StepAction::Stay)
    }
}

#[allow(dead_code)]
fn render_target_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (search, filtered, selected) = match wizard.current_step() {
        Some(WizardStep::TargetSelection {
            search,
            filtered,
            selected,
        }) => (search, filtered, *selected),
        _ => return,
    };

    // Split area into search box and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Render search box
    let search_text = format!("> {}_", search);
    let search_para = Paragraph::new(search_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title("Search Target Column"),
    );
    f.render_widget(search_para, chunks[0]);

    // Get filtered columns
    let filtered_cols: Vec<&String> = filtered
        .iter()
        .map(|&i| &wizard.data.available_columns[i])
        .collect();

    // Render filtered list
    let items: Vec<ListItem> = filtered_cols
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("  {}", col)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!("Columns ({} matches)", filtered_cols.len())),
    );

    f.render_widget(list, chunks[1]);
}

fn handle_target_selection(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    // Clone the available columns to avoid borrow checker issues
    let available_columns = wizard.data.available_columns.clone();

    let step = wizard.current_step_mut();
    let (search, filtered, selected) = match step {
        Some(WizardStep::TargetSelection {
            search,
            filtered,
            selected,
        }) => (search, filtered, selected),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) => {
            search.push(c);
            // Rebuild filtered list
            let search_lower = search.to_lowercase();
            *filtered = available_columns
                .iter()
                .enumerate()
                .filter(|(_, col)| col.to_lowercase().contains(&search_lower))
                .map(|(i, _)| i)
                .collect();
            *selected = 0;
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if !search.is_empty() {
                search.pop();
                // Rebuild filtered list
                let search_lower = search.to_lowercase();
                *filtered = available_columns
                    .iter()
                    .enumerate()
                    .filter(|(_, col)| col.to_lowercase().contains(&search_lower))
                    .map(|(i, _)| i)
                    .collect();
                *selected = 0;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Down => {
            if *selected < filtered.len().saturating_sub(1) {
                *selected += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            if !filtered.is_empty() {
                let col_index = filtered[*selected];
                wizard.data.target = Some(available_columns[col_index].clone());
                Ok(StepAction::NextStep)
            } else {
                Ok(StepAction::Stay)
            }
        }
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_target_mapping(f: &mut Frame, area: Rect, _wizard: &WizardState) {
    let text = "Target Mapping\n\n\
        Target mapping will be handled automatically during pipeline execution.\n\n\
        For binary targets (0/1), the wizard will detect and map them correctly.\n\
        For non-binary targets, you can specify event/non-event values via CLI.\n\n\
        Press Enter to continue...";

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title("Target Mapping"),
        );

    f.render_widget(paragraph, area);
}

fn handle_target_mapping(_wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    // Auto-advance on Enter
    if key.code == KeyCode::Enter {
        Ok(StepAction::NextStep)
    } else {
        Ok(StepAction::Stay)
    }
}

#[allow(dead_code)]
fn render_missing_threshold(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::MissingThreshold { input, error }) => (input, error),
        _ => return,
    };

    let mut text = format!(
        "Missing Value Threshold\n\n\
        Drop columns with missing value ratio above this threshold.\n\
        Range: 0.0 to 1.0 (e.g., 0.30 = 30%)\n\n\
        > {}_",
        input
    );

    if let Some(err) = error {
        text.push_str(&format!("\n\nError: {}", err));
    }

    let style = if error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(style)
                .title("Missing Threshold"),
        );

    f.render_widget(paragraph, area);
}

fn handle_missing_threshold(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let (input, error) = match step {
        Some(WizardStep::MissingThreshold { input, error }) => (input, error),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            input.push(c);
            *error = None;
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if !input.is_empty() {
                input.pop();
                *error = None;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => match input.parse::<f64>() {
            Ok(value) => {
                if let Err(e) = validate_threshold(value) {
                    *error = Some(e);
                    Ok(StepAction::Stay)
                } else {
                    wizard.data.missing_threshold = value;
                    Ok(StepAction::NextStep)
                }
            }
            Err(_) => {
                *error = Some("Invalid number".to_string());
                Ok(StepAction::Stay)
            }
        },
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_gini_threshold(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::GiniThreshold { input, error }) => (input, error),
        _ => return,
    };

    let mut text = format!(
        "Gini/IV Threshold\n\n\
        Drop features with Gini coefficient below this threshold.\n\
        Range: 0.0 to 1.0 (e.g., 0.05 = 5%)\n\n\
        > {}_",
        input
    );

    if let Some(err) = error {
        text.push_str(&format!("\n\nError: {}", err));
    }

    let style = if error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(style)
                .title("Gini Threshold"),
        );

    f.render_widget(paragraph, area);
}

fn handle_gini_threshold(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let (input, error) = match step {
        Some(WizardStep::GiniThreshold { input, error }) => (input, error),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            input.push(c);
            *error = None;
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if !input.is_empty() {
                input.pop();
                *error = None;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => match input.parse::<f64>() {
            Ok(value) => {
                if let Err(e) = validate_threshold(value) {
                    *error = Some(e);
                    Ok(StepAction::Stay)
                } else {
                    wizard.data.gini_threshold = value;
                    Ok(StepAction::NextStep)
                }
            }
            Err(_) => {
                *error = Some("Invalid number".to_string());
                Ok(StepAction::Stay)
            }
        },
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_correlation_threshold(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::CorrelationThreshold { input, error }) => (input, error),
        _ => return,
    };

    let mut text = format!(
        "Correlation Threshold\n\n\
        Drop one feature from pairs with correlation above this threshold.\n\
        Range: 0.0 to 1.0 (e.g., 0.40 = 40%)\n\n\
        > {}_",
        input
    );

    if let Some(err) = error {
        text.push_str(&format!("\n\nError: {}", err));
    }

    let style = if error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(style)
                .title("Correlation Threshold"),
        );

    f.render_widget(paragraph, area);
}

fn handle_correlation_threshold(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let (input, error) = match step {
        Some(WizardStep::CorrelationThreshold { input, error }) => (input, error),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            input.push(c);
            *error = None;
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if !input.is_empty() {
                input.pop();
                *error = None;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => match input.parse::<f64>() {
            Ok(value) => {
                if let Err(e) = validate_threshold(value) {
                    *error = Some(e);
                    Ok(StepAction::Stay)
                } else {
                    wizard.data.correlation_threshold = value;
                    Ok(StepAction::NextStep)
                }
            }
            Err(_) => {
                *error = Some("Invalid number".to_string());
                Ok(StepAction::Stay)
            }
        },
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_optional_settings_prompt(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let options = ["Yes", "No"];
    let selected = if wizard.optional_yes { 0 } else { 1 };

    let mut text = String::from(
        "Would you like to configure optional settings?\n\n\
        Current defaults:\n\
        - Solver: Enabled, Trend: none\n\
        - Weight: None\n\
        - Drop columns: None\n\
        - Schema: 10000 rows\n\n",
    );

    for (i, opt) in options.iter().enumerate() {
        if i == selected {
            text.push_str(&format!("  > {} <\n", opt));
        } else {
            text.push_str(&format!("    {}\n", opt));
        }
    }

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title("Optional Settings"),
        );

    f.render_widget(paragraph, area);
}

fn handle_optional_settings_prompt(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    match key.code {
        KeyCode::Up | KeyCode::Down => {
            wizard.optional_yes = !wizard.optional_yes;
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            if wizard.optional_yes {
                wizard.build_steps(); // Rebuild to insert optional steps
            }
            Ok(StepAction::NextStep)
        }
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_solver_toggle(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let selected = match wizard.current_step() {
        Some(WizardStep::SolverToggle { selected }) => *selected,
        _ => return,
    };

    let options = ["Enabled", "Disabled"];
    let current_idx = if selected { 0 } else { 1 };

    let mut text = String::from(
        "Solver Configuration\n\n\
        Use optimization solver for WoE binning?\n\n",
    );

    for (i, opt) in options.iter().enumerate() {
        if i == current_idx {
            text.push_str(&format!("  > {} <\n", opt));
        } else {
            text.push_str(&format!("    {}\n", opt));
        }
    }

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title("Solver"),
        );

    f.render_widget(paragraph, area);
}

fn handle_solver_toggle(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let selected = match step {
        Some(WizardStep::SolverToggle { selected }) => selected,
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Up | KeyCode::Down => {
            *selected = !*selected;
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            wizard.data.use_solver = *selected;
            Ok(StepAction::NextStep)
        }
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_monotonicity_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let selected = match wizard.current_step() {
        Some(WizardStep::MonotonicitySelection { selected }) => *selected,
        _ => return,
    };

    let options = [
        ("none", "No monotonicity constraint"),
        ("ascending", "WoE increases with feature value"),
        ("descending", "WoE decreases with feature value"),
        ("peak", "WoE increases then decreases"),
        ("valley", "WoE decreases then increases"),
        ("auto", "Automatically detect best constraint"),
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (name, desc))| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let content = format!("  {} - {}", name, desc);
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title("Monotonicity Constraint"),
    );

    f.render_widget(list, area);
}

fn handle_monotonicity_selection(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let selected = match step {
        Some(WizardStep::MonotonicitySelection { selected }) => selected,
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Down => {
            if *selected < 5 {
                *selected += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            let options = ["none", "ascending", "descending", "peak", "valley", "auto"];
            wizard.data.monotonicity = options[*selected].to_string();
            Ok(StepAction::NextStep)
        }
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_weight_column(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (search, filtered, selected) = match wizard.current_step() {
        Some(WizardStep::WeightColumn {
            search,
            filtered,
            selected,
        }) => (search, filtered, *selected),
        _ => return,
    };

    // Split area into search box and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Render search box
    let search_text = format!("> {}_", search);
    let search_para = Paragraph::new(search_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title("Search Weight Column"),
    );
    f.render_widget(search_para, chunks[0]);

    // Build option list: "None" + filtered columns
    let mut options = vec!["None".to_string()];
    let filtered_cols: Vec<String> = filtered
        .iter()
        .map(|&i| wizard.data.available_columns[i].clone())
        .collect();
    options.extend(filtered_cols);

    // Render filtered list
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("  {}", col)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!("Options ({} matches)", options.len())),
    );

    f.render_widget(list, chunks[1]);
}

fn handle_weight_column(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    // Clone the available columns to avoid borrow checker issues
    let available_columns = wizard.data.available_columns.clone();

    let step = wizard.current_step_mut();
    let (search, filtered, selected) = match step {
        Some(WizardStep::WeightColumn {
            search,
            filtered,
            selected,
        }) => (search, filtered, selected),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) => {
            search.push(c);
            // Rebuild filtered list
            let search_lower = search.to_lowercase();
            *filtered = available_columns
                .iter()
                .enumerate()
                .filter(|(_, col)| col.to_lowercase().contains(&search_lower))
                .map(|(i, _)| i)
                .collect();
            *selected = 0;
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if !search.is_empty() {
                search.pop();
                // Rebuild filtered list
                let search_lower = search.to_lowercase();
                *filtered = available_columns
                    .iter()
                    .enumerate()
                    .filter(|(_, col)| col.to_lowercase().contains(&search_lower))
                    .map(|(i, _)| i)
                    .collect();
                *selected = 0;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Down => {
            let max_idx = filtered.len(); // +1 for "None" option
            if *selected < max_idx {
                *selected += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            if *selected == 0 {
                wizard.data.weight_column = None;
            } else {
                let col_index = filtered[*selected - 1];
                wizard.data.weight_column = Some(available_columns[col_index].clone());
            }
            Ok(StepAction::NextStep)
        }
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_drop_columns(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (search, filtered, selected, checked) = match wizard.current_step() {
        Some(WizardStep::DropColumns {
            search,
            filtered,
            selected,
            checked,
        }) => (search, filtered, *selected, checked),
        _ => return,
    };

    // Split area into search box and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Render search box
    let search_text = format!("> {}_", search);
    let search_para = Paragraph::new(search_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title("Search Columns to Drop (Space to toggle, Enter to confirm)"),
    );
    f.render_widget(search_para, chunks[0]);

    // Get filtered columns
    let filtered_cols: Vec<&String> = filtered
        .iter()
        .map(|&i| &wizard.data.available_columns[i])
        .collect();

    // Render filtered list with checkboxes
    let items: Vec<ListItem> = filtered_cols
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let checkbox = if checked[i] { "[X]" } else { "[ ]" };
            let style = if i == selected {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("  {} {}", checkbox, col)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!("Columns ({} matches)", filtered_cols.len())),
    );

    f.render_widget(list, chunks[1]);
}

fn handle_drop_columns(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    // Clone the available columns to avoid borrow checker issues
    let available_columns = wizard.data.available_columns.clone();

    let step = wizard.current_step_mut();
    let (search, filtered, selected, checked) = match step {
        Some(WizardStep::DropColumns {
            search,
            filtered,
            selected,
            checked,
        }) => (search, filtered, selected, checked),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(' ') => {
            // Toggle checkbox
            if *selected < checked.len() {
                checked[*selected] = !checked[*selected];
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Char(c) => {
            search.push(c);
            // Rebuild filtered list
            let search_lower = search.to_lowercase();
            *filtered = available_columns
                .iter()
                .enumerate()
                .filter(|(_, col)| col.to_lowercase().contains(&search_lower))
                .map(|(i, _)| i)
                .collect();
            *selected = 0;
            // Rebuild checked list
            *checked = vec![false; filtered.len()];
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if !search.is_empty() {
                search.pop();
                // Rebuild filtered list
                let search_lower = search.to_lowercase();
                *filtered = available_columns
                    .iter()
                    .enumerate()
                    .filter(|(_, col)| col.to_lowercase().contains(&search_lower))
                    .map(|(i, _)| i)
                    .collect();
                *selected = 0;
                // Rebuild checked list
                *checked = vec![false; filtered.len()];
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Down => {
            if *selected < filtered.len().saturating_sub(1) {
                *selected += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            // Collect checked columns
            wizard.data.columns_to_drop = filtered
                .iter()
                .enumerate()
                .filter(|(i, _)| checked[*i])
                .map(|(_, &col_idx)| available_columns[col_idx].clone())
                .collect();
            Ok(StepAction::NextStep)
        }
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_schema_inference(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::SchemaInference { input, error }) => (input, error),
        _ => return,
    };

    let mut text = format!(
        "Schema Inference Length\n\n\
        Number of rows to scan for schema inference.\n\
        0 = full scan, >= 100 = sample size\n\n\
        > {}_",
        input
    );

    if let Some(err) = error {
        text.push_str(&format!("\n\nError: {}", err));
    }

    let style = if error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(style)
                .title("Schema Inference"),
        );

    f.render_widget(paragraph, area);
}

fn handle_schema_inference(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let (input, error) = match step {
        Some(WizardStep::SchemaInference { input, error }) => (input, error),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) if c.is_ascii_digit() => {
            input.push(c);
            *error = None;
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if !input.is_empty() {
                input.pop();
                *error = None;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => match input.parse::<usize>() {
            Ok(value) => {
                if let Err(e) = validate_schema_inference(value) {
                    *error = Some(e);
                    Ok(StepAction::Stay)
                } else {
                    wizard.data.infer_schema_length = value;
                    Ok(StepAction::NextStep)
                }
            }
            Err(_) => {
                *error = Some("Invalid number".to_string());
                Ok(StepAction::Stay)
            }
        },
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_summary(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let text = match wizard.data.task {
        Some(WizardTask::Reduction) => {
            let input = wizard
                .data
                .input
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "None".to_string());
            let target = wizard.data.target.as_deref().unwrap_or("None");
            let weight = wizard.data.weight_column.as_deref().unwrap_or("None");
            let drop_cols = if wizard.data.columns_to_drop.is_empty() {
                "None".to_string()
            } else {
                wizard.data.columns_to_drop.join(", ")
            };

            format!(
                "Configuration Summary - Feature Reduction\n\n\
                Input File: {}\n\
                Target: {}\n\n\
                Thresholds:\n\
                  Missing: {:.2}\n\
                  Gini: {:.2}\n\
                  Correlation: {:.2}\n\n\
                Solver: {}\n\
                Monotonicity: {}\n\
                Weight Column: {}\n\
                Drop Columns: {}\n\
                Schema Inference: {} rows\n\n\
                Press Enter to execute",
                input,
                target,
                wizard.data.missing_threshold,
                wizard.data.gini_threshold,
                wizard.data.correlation_threshold,
                if wizard.data.use_solver {
                    "Enabled"
                } else {
                    "Disabled"
                },
                wizard.data.monotonicity,
                weight,
                drop_cols,
                wizard.data.infer_schema_length
            )
        }
        Some(WizardTask::Conversion) => {
            let input = wizard
                .data
                .input
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "None".to_string());
            let output = wizard
                .data
                .conversion_output
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "None".to_string());
            let mode = if wizard.data.conversion_fast {
                "Fast (parallel)"
            } else {
                "Memory-efficient (streaming)"
            };

            format!(
                "Configuration Summary - CSV to Parquet Conversion\n\n\
                Input File: {}\n\
                Output File: {}\n\
                Conversion Mode: {}\n\
                Schema Inference: {} rows\n\n\
                Press Enter to execute",
                input, output, mode, wizard.data.infer_schema_length
            )
        }
        None => "Error: No task selected".to_string(),
    };

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green))
                .title("Summary"),
        );

    f.render_widget(paragraph, area);
}

fn handle_summary(wizard: &WizardState, key: KeyEvent) -> Result<StepAction> {
    if key.code == KeyCode::Enter {
        generate_result(wizard)
    } else {
        Ok(StepAction::Stay)
    }
}

#[allow(dead_code)]
fn render_output_path(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::OutputPath { input, error }) => (input, error),
        _ => return,
    };

    let input_file = wizard
        .data
        .input
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "None".to_string());

    let mut text = format!(
        "Output Path\n\n\
        Input: {}\n\n\
        Output file path (must end with .parquet):\n\n\
        > {}_",
        input_file, input
    );

    if let Some(err) = error {
        text.push_str(&format!("\n\nError: {}", err));
    }

    let style = if error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(style)
                .title("Output Path"),
        );

    f.render_widget(paragraph, area);
}

fn handle_output_path(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let (input, error) = match step {
        Some(WizardStep::OutputPath { input, error }) => (input, error),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) => {
            input.push(c);
            *error = None;
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if !input.is_empty() {
                input.pop();
                *error = None;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            if let Err(e) = validate_parquet_extension(input) {
                *error = Some(e);
                Ok(StepAction::Stay)
            } else {
                wizard.data.conversion_output = Some(PathBuf::from(input.clone()));
                Ok(StepAction::NextStep)
            }
        }
        _ => Ok(StepAction::Stay),
    }
}

#[allow(dead_code)]
fn render_conversion_mode(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let selected = match wizard.current_step() {
        Some(WizardStep::ConversionMode { selected }) => *selected,
        _ => return,
    };

    let options = [
        (
            "Fast (parallel)",
            "In-memory conversion with parallel processing",
        ),
        (
            "Memory-efficient (streaming)",
            "Streaming conversion for large files",
        ),
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (name, desc))| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let content = format!("  {} - {}", name, desc);
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title("Conversion Mode"),
    );

    f.render_widget(list, area);
}

fn handle_conversion_mode(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let selected = match step {
        Some(WizardStep::ConversionMode { selected }) => selected,
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Down => {
            if *selected < 1 {
                *selected += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            wizard.data.conversion_fast = *selected == 0;
            Ok(StepAction::NextStep)
        }
        _ => Ok(StepAction::Stay),
    }
}

// ============================================================================
// Validation Functions
// ============================================================================

/// Validate threshold value (must be 0.0-1.0)
#[allow(dead_code)]
pub fn validate_threshold(value: f64) -> Result<(), String> {
    if !(0.0..=1.0).contains(&value) {
        Err("Threshold must be between 0.0 and 1.0".to_string())
    } else {
        Ok(())
    }
}

/// Validate schema inference length (must be 0 or >= 100)
#[allow(dead_code)]
pub fn validate_schema_inference(value: usize) -> Result<(), String> {
    if value != 0 && value < 100 {
        Err("Schema inference must be 0 (full scan) or >= 100 rows".to_string())
    } else {
        Ok(())
    }
}

/// Validate Parquet file extension
#[allow(dead_code)]
pub fn validate_parquet_extension(path: &str) -> Result<(), String> {
    if !path.to_lowercase().ends_with(".parquet") {
        Err("Output file must have .parquet extension".to_string())
    } else {
        Ok(())
    }
}
