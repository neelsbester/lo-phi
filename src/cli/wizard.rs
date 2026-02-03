//! Interactive TUI Wizard for Lo-phi feature reduction and file format conversion
//!
//! This module provides a step-by-step guided wizard interface that walks users through
//! configuring and executing either:
//! - Feature reduction pipeline with comprehensive parameter configuration
//! - File format conversion (CSV/SAS7BDAT to Parquet, Parquet to CSV)
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
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
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

/// Configuration for file format conversion
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ConversionConfig {
    /// Input file path (CSV, Parquet, or SAS7BDAT)
    pub input: PathBuf,
    /// Output file path (Parquet or CSV)
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

    /// Output format selection for conversion (SAS7BDAT only)
    OutputFormat { selected: usize },

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
            WizardStep::OutputFormat { .. } => "Output Format",
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
    /// Flag to force full terminal redraw (set after file selector returns)
    pub needs_redraw: bool,
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
            needs_redraw: false,
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
                let ext = self
                    .data
                    .input
                    .as_ref()
                    .and_then(|p| p.extension())
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                let mut steps = vec![WizardStep::TaskSelection];

                // Always show OutputFormat step for all conversion inputs
                steps.push(WizardStep::OutputFormat { selected: 0 });

                match ext.as_str() {
                    "sas7bdat" => {
                        // SAS7BDAT: always fast (in-memory), output chosen in OutputFormat step
                        self.data.conversion_fast = true;
                    }
                    "csv" => {
                        // CSV->Parquet: show conversion mode (fast vs streaming)
                        steps.push(WizardStep::ConversionMode { selected: 0 });
                    }
                    "parquet" => {
                        // Parquet->CSV: always fast
                        self.data.conversion_fast = true;
                    }
                    _ => {
                        // Unknown: default with mode selection
                        steps.push(WizardStep::ConversionMode { selected: 0 });
                    }
                }

                steps.push(WizardStep::Summary);
                self.steps = steps;
            }
        }
    }

    /// Set conversion output path by replacing the input file's extension
    fn auto_generate_conversion_output(&mut self, ext: &str) {
        if let Some(input) = &self.data.input {
            self.data.conversion_output = Some(input.with_extension(ext));
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
        // Force full redraw if terminal was torn down (e.g. after file selector)
        if wizard.needs_redraw {
            terminal.clear()?;
            wizard.needs_redraw = false;
        }

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

fn handle_step_event(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    // Dispatch to step-specific handlers (Backspace handled per-step)
    let step = wizard.current_step().cloned();
    match step {
        Some(WizardStep::TaskSelection) => handle_task_selection(wizard, key),
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
        Some(WizardStep::OutputFormat { .. }) => handle_output_format(wizard, key),
        Some(WizardStep::ConversionMode { .. }) => handle_conversion_mode(wizard, key),
        None => Ok(StepAction::Stay),
    }
}
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
                infer_schema_length: 0, // Full scan for conversion
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
// Rendering Helpers
// ============================================================================

/// Create a centered rectangle with fixed dimensions
fn centered_fixed_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Get semantic color for a step
fn step_color(step: &WizardStep) -> Color {
    match step {
        WizardStep::TargetSelection { .. } | WizardStep::TargetMapping { .. } => Color::Magenta,
        WizardStep::MissingThreshold { .. }
        | WizardStep::GiniThreshold { .. }
        | WizardStep::CorrelationThreshold { .. }
        | WizardStep::SchemaInference { .. } => Color::Yellow,
        WizardStep::DropColumns { .. } => Color::Red,
        WizardStep::SolverToggle { .. }
        | WizardStep::MonotonicitySelection { .. }
        | WizardStep::WeightColumn { .. }
        | WizardStep::Summary => Color::Green,
        WizardStep::TaskSelection
        | WizardStep::OptionalSettingsPrompt
        | WizardStep::OutputFormat { .. }
        | WizardStep::ConversionMode { .. } => Color::Cyan,
    }
}

// ============================================================================
// Main Rendering Functions
// ============================================================================

/// Render the complete wizard UI with persistent shell layout
fn render_wizard(f: &mut Frame, wizard: &WizardState) {
    let area = f.area();

    // Logo dimensions (matching dashboard exactly)
    let logo_height = 9u16;
    let hint_height = 1u16;

    // Box dimensions (matching dashboard: 66 wide)
    let box_width = 66u16;
    let ideal_box_height = 22u16;
    let box_height =
        ideal_box_height.min(area.height.saturating_sub(logo_height + hint_height + 2));

    // Center the whole unit vertically
    let total_height = logo_height + box_height + hint_height;
    let x = area.width.saturating_sub(box_width) / 2;
    let y = area.height.saturating_sub(total_height) / 2;

    // 1. Draw logo (centered, same width as box for alignment)
    let logo_area = Rect::new(x, y, box_width.min(area.width), logo_height);
    render_logo(f, logo_area);

    // 2. Draw centered box below logo
    let box_y = y + logo_height;
    let box_area = Rect::new(x, box_y, box_width.min(area.width), box_height.max(10));
    f.render_widget(Clear, box_area);

    let color = wizard.current_step().map(step_color).unwrap_or(Color::Cyan);

    // Build step title for the box header
    let current = wizard.current_index + 1;
    let total = wizard.steps.len();
    let step_title = wizard
        .current_step()
        .map(|s| s.title())
        .unwrap_or("Unknown");
    let title_text = format!(" Step {}/{} \u{00b7} {} ", current, total, step_title);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(title_text)
        .title_style(Style::default().fg(color).bold())
        .title_alignment(Alignment::Center);

    let inner = block.inner(box_area);
    f.render_widget(block, box_area);

    // 3. Render step content inside the box
    render_step(f, inner, wizard);

    // 4. Render count indicator on bottom border (right) for list steps
    let count_text = match wizard.current_step() {
        Some(WizardStep::TargetSelection {
            filtered, selected, ..
        }) => {
            if !filtered.is_empty() {
                Some(format!(" {}/{} columns ", selected + 1, filtered.len()))
            } else {
                None
            }
        }
        Some(WizardStep::WeightColumn {
            filtered, selected, ..
        }) => {
            let total_opts = filtered.len() + 1;
            Some(format!(" {}/{} options ", selected + 1, total_opts))
        }
        Some(WizardStep::DropColumns {
            filtered,
            selected,
            checked,
            ..
        }) => {
            let sel_count = checked.iter().filter(|&&c| c).count();
            if !filtered.is_empty() {
                Some(format!(
                    " {} sel · {}/{} ",
                    sel_count,
                    selected + 1,
                    filtered.len()
                ))
            } else {
                None
            }
        }
        Some(WizardStep::OutputFormat { selected }) => {
            let total = output_format_options(wizard.data.input.as_deref()).len();
            Some(format!(" {}/{} formats ", selected + 1, total))
        }
        _ => None,
    };
    if let Some(ct) = count_text {
        let ct_len = ct.len() as u16;
        let ct_area = Rect::new(
            box_area.x + box_area.width - ct_len - 1,
            box_area.y + box_area.height - 1,
            ct_len,
            1,
        );
        f.render_widget(
            Paragraph::new(Span::styled(ct, Style::default().fg(Color::DarkGray))),
            ct_area,
        );
    }

    // 6. Help bar below box
    let hint_y = box_area.y + box_area.height;
    let hint_area = Rect::new(x, hint_y, box_width.min(area.width), 1);
    render_help_bar(f, hint_area, wizard);

    // 7. Quit overlay
    if wizard.show_quit_confirm {
        render_quit_confirm_overlay(f, wizard);
    }
}

/// Render logo
fn render_logo(f: &mut Frame, area: Rect) {
    let logo_lines = vec![
        Line::from(Span::styled(
            "██╗      ██████╗       ██████╗ ██╗  ██╗██╗",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "██║     ██╔═══██╗      ██╔══██╗██║  ██║██║",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "██║     ██║   ██║█████╗██████╔╝███████║██║",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "██║     ██║   ██║╚════╝██╔═══╝ ██╔══██║██║",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "███████╗╚██████╔╝      ██║     ██║  ██║██║",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(Span::styled(
            "╚══════╝ ╚═════╝       ╚═╝     ╚═╝  ╚═╝╚═╝",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("φ ", Style::default().fg(Color::Magenta).bold()),
            Span::styled(
                "Feature Reduction as simple as phi",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];

    let logo_paragraph = Paragraph::new(logo_lines).alignment(Alignment::Center);
    f.render_widget(logo_paragraph, area);
}

/// Render the current step inside the shell box
fn render_step(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let step = match wizard.current_step() {
        Some(s) => s,
        None => {
            let text = "Error: No current step";
            let paragraph = Paragraph::new(text).alignment(Alignment::Center);
            f.render_widget(paragraph, area);
            return;
        }
    };

    // Dispatch to step-specific renderer
    match step {
        WizardStep::TaskSelection => render_task_selection(f, area, wizard),
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
        WizardStep::OutputFormat { .. } => render_output_format(f, area, wizard),
        WizardStep::ConversionMode { .. } => render_conversion_mode(f, area, wizard),
    }
}

/// Render help bar with context-appropriate shortcuts
fn render_help_bar(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let step = wizard.current_step();
    let is_drop = matches!(step, Some(WizardStep::DropColumns { .. }));
    let is_input = matches!(
        step,
        Some(WizardStep::MissingThreshold { .. })
            | Some(WizardStep::GiniThreshold { .. })
            | Some(WizardStep::CorrelationThreshold { .. })
            | Some(WizardStep::SchemaInference { .. })
    );
    let has_search = matches!(
        step,
        Some(WizardStep::TargetSelection { .. })
            | Some(WizardStep::WeightColumn { .. })
            | Some(WizardStep::DropColumns { .. })
    );

    let mut spans = vec![];

    if wizard.is_last_step() {
        spans.push(Span::styled("  Enter", Style::default().fg(Color::Cyan)));
        spans.push(Span::styled(
            " execute  ",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        spans.push(Span::styled("  Enter", Style::default().fg(Color::Cyan)));
        spans.push(Span::styled(
            " next  ",
            Style::default().fg(Color::DarkGray),
        ));
    }

    if is_drop {
        spans.push(Span::styled("Space", Style::default().fg(Color::Cyan)));
        spans.push(Span::styled(
            " toggle  ",
            Style::default().fg(Color::DarkGray),
        ));
    }

    if has_search {
        spans.push(Span::styled("Type", Style::default().fg(Color::Cyan)));
        spans.push(Span::styled(
            " search  ",
            Style::default().fg(Color::DarkGray),
        ));
    }

    if wizard.current_index > 0 {
        if is_input || has_search {
            spans.push(Span::styled("Bksp", Style::default().fg(Color::Cyan)));
            spans.push(Span::styled(
                " delete/back  ",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::styled("Bksp", Style::default().fg(Color::Cyan)));
            spans.push(Span::styled(
                " back  ",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    spans.push(Span::styled("Q/Esc", Style::default().fg(Color::Cyan)));
    spans.push(Span::styled(" quit", Style::default().fg(Color::DarkGray)));

    let help_line = Line::from(spans);
    let paragraph = Paragraph::new(help_line).alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

/// Render quit confirmation overlay
fn render_quit_confirm_overlay(f: &mut Frame, _wizard: &WizardState) {
    let popup = centered_fixed_rect(40, 8, f.area());
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(" Quit Wizard? ")
        .title_style(Style::default().fg(Color::Red).bold())
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Are you sure you want to quit?",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("      ", Style::default()),
            Span::styled("Y", Style::default().fg(Color::Cyan)),
            Span::styled(" yes  ", Style::default().fg(Color::DarkGray)),
            Span::styled("N", Style::default().fg(Color::Cyan)),
            Span::styled(" no", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let paragraph = Paragraph::new(content);
    f.render_widget(paragraph, inner);
}

// ============================================================================
// Step Renderers
// ============================================================================

/// Render threshold content helper (no border, renders inside provided area)
fn render_threshold_content(
    f: &mut Frame,
    area: Rect,
    title: &str,
    description: &str,
    input: &str,
    error: &Option<String>,
) {
    let color = Color::Yellow;
    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", title),
            Style::default().fg(Color::DarkGray).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", description),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Value: ", Style::default().fg(Color::DarkGray)),
            Span::styled(input.to_string(), Style::default().fg(Color::White).bold()),
            Span::styled("\u{258c}", Style::default().fg(color)),
        ]),
    ];

    if let Some(err) = error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            format!("  {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    f.render_widget(Paragraph::new(content), area);
}

fn render_task_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let options = ["Reduce features", "Convert format (csv, parquet, sas7bdat)"];
    let color = Color::Cyan;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title area
            Constraint::Min(1),    // List area
        ])
        .split(area);

    let title = Paragraph::new(Line::from(Span::styled(
        "  What would you like to do?",
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == wizard.task_selected_index {
                Style::default().fg(Color::Black).bg(color).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", opt)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(wizard.task_selected_index));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_target_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (search, filtered, selected) = match wizard.current_step() {
        Some(WizardStep::TargetSelection {
            search,
            filtered,
            selected,
        }) => (search, filtered, *selected),
        _ => return,
    };

    let color = Color::Magenta;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Search box
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Search ")
        .title_style(Style::default().fg(Color::DarkGray));

    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search.to_string(), Style::default().fg(Color::White)),
        Span::styled("\u{258c}", Style::default().fg(color)),
    ]))
    .block(search_block);
    f.render_widget(search_para, chunks[0]);

    // Column list with scrolling
    let filtered_cols: Vec<&String> = filtered
        .iter()
        .map(|&i| &wizard.data.available_columns[i])
        .collect();

    let max_visible = chunks[1].height as usize;
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = filtered_cols
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, col)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(color).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", col)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_target_mapping(f: &mut Frame, area: Rect, _wizard: &WizardState) {
    let content = vec![
        Line::from(""),
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            "  Binary Outcome - No mapping required",
            Style::default().fg(Color::Magenta).bold(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(" to continue", Style::default().fg(Color::DarkGray)),
        ]),
    ];
    f.render_widget(Paragraph::new(content), area);
}

fn render_missing_threshold(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::MissingThreshold { input, error }) => (input, error),
        _ => return,
    };
    render_threshold_content(
        f,
        area,
        "Missing Threshold",
        "Drop columns exceeding null ratio (0.0-1.0)",
        input,
        error,
    );
}

fn render_gini_threshold(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::GiniThreshold { input, error }) => (input, error),
        _ => return,
    };
    render_threshold_content(
        f,
        area,
        "Gini Threshold",
        "Drop features with Gini below threshold (0.0-1.0)",
        input,
        error,
    );
}

fn render_correlation_threshold(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::CorrelationThreshold { input, error }) => (input, error),
        _ => return,
    };
    render_threshold_content(
        f,
        area,
        "Correlation Threshold",
        "Drop correlated features above threshold (0.0-1.0)",
        input,
        error,
    );
}

fn render_optional_settings_prompt(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let color = Color::Cyan;
    let selected = if wizard.optional_yes { 0 } else { 1 };
    let options = ["Yes - configure advanced settings", "No - use defaults"];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(1)])
        .split(area);

    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Configure optional settings?",
            Style::default().fg(Color::DarkGray).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Defaults: Solver enabled, Trend none, Weight none",
            Style::default().fg(Color::DarkGray),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(color).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", opt)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_solver_toggle(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let selected = match wizard.current_step() {
        Some(WizardStep::SolverToggle { selected }) => *selected,
        _ => return,
    };
    let color = Color::Green;
    let current_idx = if selected { 0 } else { 1 };
    let options = ["Enabled", "Disabled"];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(area);

    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Solver Configuration",
            Style::default().fg(Color::DarkGray).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Use optimization solver for WoE binning?",
            Style::default().fg(Color::DarkGray),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == current_idx {
                Style::default().fg(Color::Black).bg(color).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", opt)).style(style)
        })
        .collect();
    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(current_idx));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_monotonicity_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let selected = match wizard.current_step() {
        Some(WizardStep::MonotonicitySelection { selected }) => *selected,
        _ => return,
    };
    let color = Color::Green;
    let options = ["none", "ascending", "descending", "peak", "valley", "auto"];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Monotonicity Constraint",
            Style::default().fg(Color::DarkGray).bold(),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(color).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", opt)).style(style)
        })
        .collect();
    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_weight_column(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (search, filtered, selected) = match wizard.current_step() {
        Some(WizardStep::WeightColumn {
            search,
            filtered,
            selected,
        }) => (search, filtered, *selected),
        _ => return,
    };
    let color = Color::Green;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Search ")
        .title_style(Style::default().fg(Color::DarkGray));
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search.to_string(), Style::default().fg(Color::White)),
        Span::styled("\u{258c}", Style::default().fg(color)),
    ]))
    .block(search_block);
    f.render_widget(search_para, chunks[0]);

    let filtered_cols: Vec<String> = filtered
        .iter()
        .map(|&i| wizard.data.available_columns[i].clone())
        .collect();
    let mut options = vec!["(None)".to_string()];
    options.extend(filtered_cols);

    let max_visible = chunks[1].height as usize;
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, col)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(color).bold()
            } else if i == 0 {
                Style::default().fg(Color::DarkGray).italic()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", col)).style(style)
        })
        .collect();
    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

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
    let color = Color::Red;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Search ")
        .title_style(Style::default().fg(Color::DarkGray));
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search.to_string(), Style::default().fg(Color::White)),
        Span::styled("\u{258c}", Style::default().fg(color)),
    ]))
    .block(search_block);
    f.render_widget(search_para, chunks[0]);

    let filtered_cols: Vec<&String> = filtered
        .iter()
        .map(|&i| &wizard.data.available_columns[i])
        .collect();

    let max_visible = chunks[1].height as usize;
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = filtered_cols
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, col)| {
            let is_checked = checked.get(i).copied().unwrap_or(false);
            let checkbox = if is_checked { "[x]" } else { "[ ]" };
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(color).bold()
            } else if is_checked {
                Style::default().fg(color)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {} {}", checkbox, col)).style(style)
        })
        .collect();
    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_schema_inference(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::SchemaInference { input, error }) => (input, error),
        _ => return,
    };
    render_threshold_content(
        f,
        area,
        "Schema Inference",
        "Rows for schema inference (0 = full scan, >= 100)",
        input,
        error,
    );
}

fn render_summary(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let color = Color::Green;
    let content = match wizard.data.task {
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
            let solver = if wizard.data.use_solver {
                "Enabled"
            } else {
                "Disabled"
            };

            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Configuration Summary",
                    Style::default().fg(Color::DarkGray).bold(),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Input:        ", Style::default().fg(Color::DarkGray)),
                    Span::styled(input, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Target:       ", Style::default().fg(Color::DarkGray)),
                    Span::styled(target.to_string(), Style::default().fg(color)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Missing:      ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{:.2}", wizard.data.missing_threshold),
                        Style::default().fg(color),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Gini:         ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{:.2}", wizard.data.gini_threshold),
                        Style::default().fg(color),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Correlation:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{:.2}", wizard.data.correlation_threshold),
                        Style::default().fg(color),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Solver:       ", Style::default().fg(Color::DarkGray)),
                    Span::styled(solver.to_string(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Monotonicity: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(wizard.data.monotonicity.clone(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Weight:       ", Style::default().fg(Color::DarkGray)),
                    Span::styled(weight.to_string(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Drop:         ", Style::default().fg(Color::DarkGray)),
                    Span::styled(drop_cols, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Schema:       ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{} rows", wizard.data.infer_schema_length),
                        Style::default().fg(color),
                    ),
                ]),
            ]
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

            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Configuration Summary",
                    Style::default().fg(Color::DarkGray).bold(),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Input:   ", Style::default().fg(Color::DarkGray)),
                    Span::styled(input, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Output:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(output, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Mode:    ", Style::default().fg(Color::DarkGray)),
                    Span::styled(mode.to_string(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Schema:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("All rows (full scan)", Style::default().fg(color)),
                ]),
            ]
        }
        None => vec![Line::from(Span::styled(
            "Error: No task selected",
            Style::default().fg(Color::Red),
        ))],
    };
    f.render_widget(Paragraph::new(content), area);
}

/// Get available output format options based on input file extension
fn output_format_options(input: Option<&std::path::Path>) -> Vec<&'static str> {
    let ext = input
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "csv" => vec!["Parquet (.parquet)"],
        "parquet" => vec!["CSV (.csv)"],
        _ => vec!["Parquet (.parquet)", "CSV (.csv)"], // SAS7BDAT and unknown
    }
}

/// Map output format option label to file extension
fn output_format_extension(option: &str) -> &str {
    if option.contains("Parquet") {
        "parquet"
    } else {
        "csv"
    }
}

fn render_output_format(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let selected = match wizard.current_step() {
        Some(WizardStep::OutputFormat { selected }) => *selected,
        _ => return,
    };
    let color = Color::Cyan;
    let options = output_format_options(wizard.data.input.as_deref());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Output Format",
            Style::default().fg(Color::DarkGray).bold(),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(color).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", opt)).style(style)
        })
        .collect();
    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_conversion_mode(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let selected = match wizard.current_step() {
        Some(WizardStep::ConversionMode { selected }) => *selected,
        _ => return,
    };
    let color = Color::Cyan;
    let options = ["Fast (parallel)", "Memory-efficient (streaming)"];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Conversion Mode",
            Style::default().fg(Color::DarkGray).bold(),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(color).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("  {}", opt)).style(style)
        })
        .collect();
    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

// ============================================================================
// Event Handlers
// ============================================================================

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

            // Open file selector immediately (skip intermediate FileSelection step)
            if wizard.data.input.is_none() {
                teardown_terminal();
                let result = super::config_menu::run_file_selector()?;
                enable_raw_mode()?;
                stdout().execute(EnterAlternateScreen)?;
                // Force full redraw since terminal buffer is stale after file selector
                wizard.needs_redraw = true;

                match result {
                    super::config_menu::FileSelectResult::Selected(path) => {
                        wizard.data.available_columns = crate::pipeline::get_column_names(&path)?;
                        wizard.data.input = Some(path);
                    }
                    super::config_menu::FileSelectResult::Cancelled => {
                        // Revert task selection, stay on this step
                        wizard.data.task = None;
                        return Ok(StepAction::Stay);
                    }
                }
            } else if let Some(input) = &wizard.data.input {
                // File pre-populated from CLI, just load columns
                wizard.data.available_columns = crate::pipeline::get_column_names(input)?;
            }

            wizard.build_steps();
            Ok(StepAction::NextStep)
        }
        KeyCode::Backspace => Ok(StepAction::PrevStep),
        _ => Ok(StepAction::Stay),
    }
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
            if search.is_empty() {
                return Ok(StepAction::PrevStep);
            }
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

fn handle_target_mapping(_wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    match key.code {
        KeyCode::Enter => Ok(StepAction::NextStep),
        KeyCode::Backspace => Ok(StepAction::PrevStep),
        _ => Ok(StepAction::Stay),
    }
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
            if input.is_empty() {
                return Ok(StepAction::PrevStep);
            }
            input.pop();
            *error = None;
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
            if input.is_empty() {
                return Ok(StepAction::PrevStep);
            }
            input.pop();
            *error = None;
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
            if input.is_empty() {
                return Ok(StepAction::PrevStep);
            }
            input.pop();
            *error = None;
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
        KeyCode::Backspace => Ok(StepAction::PrevStep),
        _ => Ok(StepAction::Stay),
    }
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
        KeyCode::Backspace => Ok(StepAction::PrevStep),
        _ => Ok(StepAction::Stay),
    }
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
        KeyCode::Backspace => Ok(StepAction::PrevStep),
        _ => Ok(StepAction::Stay),
    }
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
            if search.is_empty() {
                return Ok(StepAction::PrevStep);
            }
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
            if search.is_empty() {
                return Ok(StepAction::PrevStep);
            }
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
            if input.is_empty() {
                return Ok(StepAction::PrevStep);
            }
            input.pop();
            *error = None;
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

fn handle_summary(wizard: &WizardState, key: KeyEvent) -> Result<StepAction> {
    match key.code {
        KeyCode::Enter => generate_result(wizard),
        KeyCode::Backspace => Ok(StepAction::PrevStep),
        _ => Ok(StepAction::Stay),
    }
}

fn handle_output_format(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let options = output_format_options(wizard.data.input.as_deref());
    let max_idx = options.len().saturating_sub(1);

    let step = wizard.current_step_mut();
    let selected = match step {
        Some(WizardStep::OutputFormat { selected }) => selected,
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
            if *selected < max_idx {
                *selected += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            let ext = output_format_extension(options[*selected]);
            wizard.auto_generate_conversion_output(ext);
            // SAS7BDAT and Parquet are always fast; CSV fast is decided in ConversionMode step
            let input_ext = wizard
                .data
                .input
                .as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if input_ext != "csv" {
                wizard.data.conversion_fast = true;
            }
            Ok(StepAction::NextStep)
        }
        KeyCode::Backspace => Ok(StepAction::PrevStep),
        _ => Ok(StepAction::Stay),
    }
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
        KeyCode::Backspace => Ok(StepAction::PrevStep),
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

