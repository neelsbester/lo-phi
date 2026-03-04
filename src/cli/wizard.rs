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

use std::collections::HashSet;
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
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    Terminal,
};

use polars::prelude::*;

use super::args::Cli;
use super::config_menu::Config;
use super::shared::{
    check_terminal_size, draw_too_small_overlay, render_logo, themed, MIN_COLS, MIN_ROWS,
};
use super::theme;
use crate::pipeline::{
    SampleSize, SamplingConfig, SamplingMethod, StratumSpec, TargetAnalysis, TargetMapping,
};
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
    /// User completed sampling configuration
    RunSampling(Box<SamplingConfig>),
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
    /// Dataset sampling with inverse probability weights
    Sampling,
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
        /// Set of original column indices (into `available_columns`) that are checked.
        /// Keyed on original index so selections survive search filtering.
        checked: HashSet<usize>,
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

    /// Sampling method selection (Random / Stratified / Equal)
    SamplingMethodSelection { selected: usize },

    /// Sample size input (count or fraction, toggle with Tab)
    SampleSizeInput {
        input: String,
        error: Option<String>,
        is_count: bool,
    },

    /// Strata column selection with search/filter
    StrataColumnSelection {
        search: String,
        filtered: Vec<usize>,
        selected: usize,
    },

    /// Per-stratum size configuration table
    StratumSizeConfig {
        /// (value, population_count, user_input_string)
        strata: Vec<(String, usize, String)>,
        selected: usize,
        error: Option<String>,
        scroll_offset: usize,
    },

    /// Optional seed input
    SeedInput {
        input: String,
        error: Option<String>,
    },
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
            WizardStep::SamplingMethodSelection { .. } => "Sampling Method",
            WizardStep::SampleSizeInput { .. } => "Sample Size",
            WizardStep::StrataColumnSelection { .. } => "Strata Column",
            WizardStep::StratumSizeConfig { .. } => "Stratum Sizes",
            WizardStep::SeedInput { .. } => "Random Seed",
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

    // Sampling-specific fields
    pub sampling_method: Option<SamplingMethod>,
    pub sampling_strata_column: Option<String>,
    pub sampling_size: Option<SampleSize>,
    pub sampling_strata_specs: Vec<StratumSpec>,
    pub sampling_seed: Option<u64>,
    /// Loaded after strata column selection: (value, count) sorted by count desc
    pub sampling_strata_info: Vec<(String, usize)>,

    // Temporary state for multi-step processes
    pub available_columns: Vec<String>,
    pub target_unique_values: Vec<String>,
    /// True when target column is already binary 0/1 (mapping step is skipped)
    pub target_is_binary: bool,
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
            sampling_method: None,
            sampling_strata_column: None,
            sampling_size: None,
            sampling_strata_specs: Vec::new(),
            sampling_seed: None,
            sampling_strata_info: Vec::new(),
            available_columns: Vec::new(),
            target_unique_values: Vec::new(),
            target_is_binary: false,
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
    /// Scroll offset for the Summary step
    pub summary_scroll: usize,
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
            summary_scroll: 0,
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
                        checked: HashSet::new(),
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
            WizardTask::Sampling => {
                let all_indices: Vec<usize> = (0..self.data.available_columns.len()).collect();
                let mut steps = vec![
                    WizardStep::TaskSelection,
                    WizardStep::SamplingMethodSelection { selected: 0 },
                ];

                let method = self
                    .data
                    .sampling_method
                    .clone()
                    .unwrap_or(SamplingMethod::Random);

                match method {
                    SamplingMethod::Random => {
                        steps.push(WizardStep::SampleSizeInput {
                            input: String::new(),
                            error: None,
                            is_count: true,
                        });
                    }
                    SamplingMethod::Stratified => {
                        steps.push(WizardStep::StrataColumnSelection {
                            search: String::new(),
                            filtered: all_indices,
                            selected: 0,
                        });
                        // StratumSizeConfig is populated after column selection
                        let strata_data: Vec<(String, usize, String)> = self
                            .data
                            .sampling_strata_info
                            .iter()
                            .map(|(v, c)| (v.clone(), *c, String::new()))
                            .collect();
                        if !strata_data.is_empty() {
                            steps.push(WizardStep::StratumSizeConfig {
                                strata: strata_data,
                                selected: 0,
                                error: None,
                                scroll_offset: 0,
                            });
                        }
                    }
                    SamplingMethod::EqualAllocation => {
                        steps.push(WizardStep::StrataColumnSelection {
                            search: String::new(),
                            filtered: all_indices,
                            selected: 0,
                        });
                        steps.push(WizardStep::SampleSizeInput {
                            input: String::new(),
                            error: None,
                            is_count: true,
                        });
                    }
                }

                steps.push(WizardStep::SeedInput {
                    input: String::new(),
                    error: None,
                });
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

/// Run the wizard interface, tearing down the TUI before returning.
///
/// When the user chooses RunReduction, the TUI is left active and the live
/// terminal is returned via `run_wizard_keep_tui` instead.  This variant
/// always tears down the terminal and is used when the caller does not need
/// the progress overlay (e.g. conversion workflow, or `--no-confirm` mode).
#[allow(dead_code)]
pub fn run_wizard(cli: &Cli) -> Result<WizardResult> {
    let (result, _terminal) = run_wizard_impl(cli)?;
    Ok(result)
}

/// Run the wizard interface and, on `RunReduction`, return the still-active
/// `Terminal` so that the caller can display the progress overlay without
/// tearing down and re-entering alternate screen.
///
/// On `Quit` the terminal is torn down before this function returns and the
/// `Option<Terminal>` is `None`.
pub fn run_wizard_keep_tui(
    cli: &Cli,
) -> Result<(WizardResult, Option<Terminal<CrosstermBackend<Stdout>>>)> {
    run_wizard_impl(cli)
}

fn run_wizard_impl(
    cli: &Cli,
) -> Result<(WizardResult, Option<Terminal<CrosstermBackend<Stdout>>>)> {
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

    // Check terminal size before entering TUI
    if let Err(msg) = check_terminal_size() {
        eprintln!("{}", msg);
        return Ok((WizardResult::Quit, None));
    }

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Run wizard loop -- capture result so teardown always runs before propagating error
    let result = run_wizard_loop(&mut terminal, &mut wizard);

    match result {
        Ok(WizardResult::RunReduction(cfg)) => {
            // Keep TUI alive — caller will display the progress overlay
            Ok((WizardResult::RunReduction(cfg), Some(terminal)))
        }
        Ok(WizardResult::RunSampling(cfg)) => {
            // Keep TUI alive — caller will display the sampling progress overlay
            Ok((WizardResult::RunSampling(cfg), Some(terminal)))
        }
        Ok(WizardResult::RunConversion(cfg)) => {
            // Keep TUI alive — caller will display the conversion progress overlay
            Ok((WizardResult::RunConversion(cfg), Some(terminal)))
        }
        Ok(other) => {
            teardown_terminal();
            Ok((other, None))
        }
        Err(e) => {
            teardown_terminal();
            Err(e)
        }
    }
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
        // Check current terminal size
        let (cols, rows) = crossterm::terminal::size().unwrap_or((0, 0));
        let terminal_too_small = cols < MIN_COLS || rows < MIN_ROWS;

        // Force full redraw if terminal was torn down (e.g. after file selector)
        if wizard.needs_redraw {
            terminal.clear()?;
            wizard.needs_redraw = false;
        }

        // Draw current state
        terminal.draw(|f| {
            if terminal_too_small {
                draw_too_small_overlay(f);
            } else {
                render_wizard(f, wizard);
            }
        })?;

        // Poll for events
        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Resize(_, _) => {
                    // Size flag is updated at the top of each loop iteration
                    continue;
                }
                Event::Key(key) => {
                    // Only handle key press events, not release
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    // Ignore keypresses while terminal is too small (except quit)
                    if terminal_too_small {
                        if matches!(
                            key.code,
                            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                        ) {
                            return Ok(WizardResult::Quit);
                        }
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
                _ => {}
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
        Some(WizardStep::SamplingMethodSelection { .. }) => {
            handle_sampling_method_selection(wizard, key)
        }
        Some(WizardStep::SampleSizeInput { .. }) => handle_sample_size_input(wizard, key),
        Some(WizardStep::StrataColumnSelection { .. }) => {
            handle_strata_column_selection(wizard, key)
        }
        Some(WizardStep::StratumSizeConfig { .. }) => handle_stratum_size_config(wizard, key),
        Some(WizardStep::SeedInput { .. }) => handle_seed_input(wizard, key),
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
        Some(WizardTask::Sampling) => {
            let input = wizard
                .data
                .input
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No input file selected"))?;
            let method = wizard
                .data
                .sampling_method
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No sampling method selected"))?;

            let output = generate_output_path(&input, "_sampled")?;

            let config = SamplingConfig {
                input,
                output,
                method: method.clone(),
                strata_column: wizard.data.sampling_strata_column.clone(),
                sample_size: wizard.data.sampling_size.clone(),
                strata_specs: wizard.data.sampling_strata_specs.clone(),
                seed: wizard.data.sampling_seed,
                infer_schema_length: wizard.data.infer_schema_length,
            };

            Ok(StepAction::Complete(WizardResult::RunSampling(Box::new(
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
        WizardStep::TargetSelection { .. } | WizardStep::TargetMapping { .. } => theme::ACCENT,
        WizardStep::MissingThreshold { .. }
        | WizardStep::GiniThreshold { .. }
        | WizardStep::CorrelationThreshold { .. }
        | WizardStep::SchemaInference { .. } => theme::WARNING,
        WizardStep::DropColumns { .. } => theme::ERROR,
        WizardStep::SolverToggle { .. }
        | WizardStep::MonotonicitySelection { .. }
        | WizardStep::WeightColumn { .. }
        | WizardStep::Summary => theme::SUCCESS,
        WizardStep::TaskSelection
        | WizardStep::OptionalSettingsPrompt
        | WizardStep::OutputFormat { .. }
        | WizardStep::ConversionMode { .. }
        | WizardStep::SamplingMethodSelection { .. } => theme::PRIMARY,
        WizardStep::SampleSizeInput { .. } | WizardStep::StratumSizeConfig { .. } => theme::WARNING,
        WizardStep::StrataColumnSelection { .. } => theme::ACCENT,
        WizardStep::SeedInput { .. } => theme::SUCCESS,
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

    let color = wizard
        .current_step()
        .map(step_color)
        .unwrap_or(theme::PRIMARY);

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
        .border_style(themed(Style::default().fg(color)))
        .title(title_text)
        .title_style(themed(Style::default().fg(color).bold()))
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
            let sel_count = checked.len();
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
        Some(WizardStep::TargetMapping {
            unique_values,
            event_selected,
            focus: TargetMappingFocus::Event,
            ..
        }) if !unique_values.is_empty() => {
            let sel = event_selected.unwrap_or(0);
            Some(format!(" {}/{} values ", sel + 1, unique_values.len()))
        }
        Some(WizardStep::TargetMapping {
            unique_values,
            non_event_selected,
            focus: TargetMappingFocus::NonEvent,
            ..
        }) if !unique_values.is_empty() => {
            let remaining_len = unique_values.len().saturating_sub(1);
            let sel = non_event_selected.unwrap_or(0);
            Some(format!(" {}/{} values ", sel + 1, remaining_len))
        }
        Some(WizardStep::StrataColumnSelection {
            filtered, selected, ..
        }) => {
            if !filtered.is_empty() {
                Some(format!(" {}/{} columns ", selected + 1, filtered.len()))
            } else {
                None
            }
        }
        Some(WizardStep::StratumSizeConfig {
            strata, selected, ..
        }) => {
            if !strata.is_empty() {
                Some(format!(" {}/{} strata ", selected + 1, strata.len()))
            } else {
                None
            }
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
            Paragraph::new(Span::styled(ct, Style::default().fg(theme::MUTED))),
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
        WizardStep::SamplingMethodSelection { .. } => {
            render_sampling_method_selection(f, area, wizard)
        }
        WizardStep::SampleSizeInput { .. } => render_sample_size_input(f, area, wizard),
        WizardStep::StrataColumnSelection { .. } => render_strata_column_selection(f, area, wizard),
        WizardStep::StratumSizeConfig { .. } => render_stratum_size_config(f, area, wizard),
        WizardStep::SeedInput { .. } => render_seed_input(f, area, wizard),
        WizardStep::Summary => render_summary(f, area, wizard),
        WizardStep::OutputFormat { .. } => render_output_format(f, area, wizard),
        WizardStep::ConversionMode { .. } => render_conversion_mode(f, area, wizard),
    }
}

/// Render help bar with context-appropriate shortcuts
fn render_help_bar(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let step = wizard.current_step();
    let is_drop = matches!(step, Some(WizardStep::DropColumns { .. }));
    let is_summary = matches!(step, Some(WizardStep::Summary));
    let is_input = matches!(
        step,
        Some(WizardStep::MissingThreshold { .. })
            | Some(WizardStep::GiniThreshold { .. })
            | Some(WizardStep::CorrelationThreshold { .. })
            | Some(WizardStep::SchemaInference { .. })
            | Some(WizardStep::SampleSizeInput { .. })
            | Some(WizardStep::SeedInput { .. })
    );
    let has_search = matches!(
        step,
        Some(WizardStep::TargetSelection { .. })
            | Some(WizardStep::WeightColumn { .. })
            | Some(WizardStep::DropColumns { .. })
            | Some(WizardStep::StrataColumnSelection { .. })
    );
    let is_stratum_config = matches!(step, Some(WizardStep::StratumSizeConfig { .. }));
    let is_target_mapping_non_binary = matches!(step, Some(WizardStep::TargetMapping { .. }))
        && !wizard.data.target_is_binary
        && !wizard.data.target_unique_values.is_empty();
    let target_mapping_in_non_event = is_target_mapping_non_binary
        && matches!(
            step,
            Some(WizardStep::TargetMapping {
                focus: TargetMappingFocus::NonEvent,
                ..
            })
        );

    // Key style: Blue when colors are on, plain when NO_COLOR is active
    let key_style = themed(Style::default().fg(theme::KEYS));
    let desc_style = Style::default().fg(theme::MUTED);

    let mut spans = vec![];

    if is_target_mapping_non_binary {
        // Custom help bar for the two-phase target mapping UI
        if target_mapping_in_non_event {
            spans.push(Span::styled("  Enter", key_style));
            spans.push(Span::styled(" confirm non-event  ", desc_style));
            spans.push(Span::styled("Bksp", key_style));
            spans.push(Span::styled(" back to event  ", desc_style));
        } else {
            spans.push(Span::styled("  Enter", key_style));
            spans.push(Span::styled(" select event  ", desc_style));
            spans.push(Span::styled("↑/↓", key_style));
            spans.push(Span::styled(" navigate  ", desc_style));
            if wizard.current_index > 0 {
                spans.push(Span::styled("Bksp", key_style));
                spans.push(Span::styled(" back  ", desc_style));
            }
        }
        spans.push(Span::styled("Q/Esc", key_style));
        spans.push(Span::styled(" quit", desc_style));
    } else {
        if wizard.is_last_step() {
            spans.push(Span::styled("  Enter", key_style));
            spans.push(Span::styled(" execute  ", desc_style));
        } else {
            spans.push(Span::styled("  Enter", key_style));
            spans.push(Span::styled(" next  ", desc_style));
        }

        if is_drop {
            spans.push(Span::styled("Space", key_style));
            spans.push(Span::styled(" toggle  ", desc_style));
        }

        if has_search {
            spans.push(Span::styled("Type", key_style));
            spans.push(Span::styled(" search  ", desc_style));
        }

        if is_summary || is_stratum_config {
            spans.push(Span::styled("↑/↓", key_style));
            spans.push(Span::styled(" navigate  ", desc_style));
        }

        if is_stratum_config {
            spans.push(Span::styled("Type", key_style));
            spans.push(Span::styled(" digits  ", desc_style));
        }

        if wizard.current_index > 0 {
            if is_input || has_search {
                spans.push(Span::styled("Bksp", key_style));
                spans.push(Span::styled(" delete/back  ", desc_style));
            } else {
                spans.push(Span::styled("Bksp", key_style));
                spans.push(Span::styled(" back  ", desc_style));
            }
        }

        spans.push(Span::styled("Q/Esc", key_style));
        spans.push(Span::styled(" quit", desc_style));
    }

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
        .border_style(Style::default().fg(theme::DANGER))
        .title(" Quit Wizard? ")
        .title_style(Style::default().fg(theme::DANGER).bold())
        .style(Style::default().bg(theme::BASE));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Are you sure you want to quit?",
            Style::default().fg(theme::TEXT),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("      ", Style::default()),
            Span::styled("Y", Style::default().fg(theme::KEYS)),
            Span::styled(" yes  ", Style::default().fg(theme::MUTED)),
            Span::styled("N", Style::default().fg(theme::KEYS)),
            Span::styled(" no", Style::default().fg(theme::MUTED)),
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
    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", title),
            Style::default().fg(theme::SUBTEXT).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", description),
            Style::default().fg(theme::MUTED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Value: ", Style::default().fg(theme::MUTED)),
            Span::styled(input.to_string(), Style::default().fg(theme::TEXT).bold()),
            Span::styled("\u{258c}", Style::default().fg(theme::WARNING)),
        ]),
    ];

    if let Some(err) = error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            format!("  {}", err),
            Style::default().fg(theme::ERROR),
        )));
    }

    f.render_widget(Paragraph::new(content), area);
}

fn render_task_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let options = [
        "Reduce features",
        "Convert format (csv, parquet, sas7bdat)",
        "Sample dataset",
    ];
    let color = theme::PRIMARY;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title area
            Constraint::Min(1),    // List area
        ])
        .split(area);

    let title = Paragraph::new(Line::from(Span::styled(
        "  What would you like to do?",
        Style::default().fg(theme::MUTED),
    )));
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == wizard.task_selected_index {
                Style::default().fg(theme::BASE).bg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
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

    let color = theme::ACCENT;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Search box
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SURFACE))
        .title(" Search ")
        .title_style(Style::default().fg(theme::SURFACE));

    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search.to_string(), Style::default().fg(theme::TEXT)),
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
                Style::default().fg(theme::BASE).bg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
            };
            ListItem::new(format!("  {}", col)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_target_mapping(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let color = theme::ACCENT;

    // Binary target: no mapping needed
    if wizard.data.target_is_binary || wizard.data.target_unique_values.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled(
                "  Binary target detected - no mapping required",
                themed(Style::default().fg(color).bold()),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Press ", Style::default().fg(theme::MUTED)),
                Span::styled("Enter", themed(Style::default().fg(theme::KEYS))),
                Span::styled(" to continue", Style::default().fg(theme::MUTED)),
            ]),
        ];
        f.render_widget(Paragraph::new(content), area);
        return;
    }

    // Non-binary: two-phase selection
    let (unique_values, event_selected, non_event_selected, focus) = match wizard.current_step() {
        Some(WizardStep::TargetMapping {
            unique_values,
            event_selected,
            non_event_selected,
            focus,
        }) => (unique_values, event_selected, non_event_selected, focus),
        _ => return,
    };

    match focus {
        TargetMappingFocus::Event => {
            render_target_event_selection(f, area, unique_values, *event_selected, color);
        }
        TargetMappingFocus::NonEvent => {
            let event_idx = event_selected.unwrap_or(0);
            let event_value = unique_values
                .get(event_idx)
                .map(|s| s.as_str())
                .unwrap_or("");
            let remaining: Vec<String> = unique_values
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != event_idx)
                .map(|(_, v)| v.clone())
                .collect();
            render_target_non_event_selection(
                f,
                area,
                &remaining,
                *non_event_selected,
                event_value,
                color,
            );
        }
    }
}

fn render_target_event_selection(
    f: &mut Frame,
    area: Rect,
    unique_values: &[String],
    selected: Option<usize>,
    color: Color,
) {
    let selected = selected.unwrap_or(0);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(area);

    let header = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Step 1 of 2 - Select EVENT value (1)",
            themed(Style::default().fg(color).bold()),
        )),
        Line::from(Span::styled(
            "  Which value represents the positive outcome?",
            Style::default().fg(theme::MUTED),
        )),
    ]);
    f.render_widget(header, chunks[0]);

    let max_visible = chunks[1].height as usize;
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = unique_values
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, value)| {
            let style = if i == selected {
                themed(Style::default().fg(theme::BASE).bg(color).bold())
            } else {
                Style::default().fg(theme::TEXT)
            };
            ListItem::new(format!("  {}", value)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_target_non_event_selection(
    f: &mut Frame,
    area: Rect,
    remaining: &[String],
    selected: Option<usize>,
    event_value: &str,
    color: Color,
) {
    let selected = selected.unwrap_or(0);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(1)])
        .split(area);

    let header = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Step 2 of 2 - Select NON-EVENT value (0)",
            themed(Style::default().fg(theme::WARNING).bold()),
        )),
        Line::from(vec![
            Span::styled("  Event (1): ", Style::default().fg(theme::MUTED)),
            Span::styled(
                event_value.to_string(),
                themed(Style::default().fg(color).bold()),
            ),
        ]),
        Line::from(Span::styled(
            "  Which value represents the negative outcome?",
            Style::default().fg(theme::MUTED),
        )),
    ]);
    f.render_widget(header, chunks[0]);

    let max_visible = chunks[1].height as usize;
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = remaining
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, value)| {
            let style = if i == selected {
                themed(Style::default().fg(theme::BASE).bg(theme::WARNING).bold())
            } else {
                Style::default().fg(theme::TEXT)
            };
            ListItem::new(format!("  {}", value)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
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
    let color = theme::PRIMARY;
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
            Style::default().fg(theme::SUBTEXT).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Defaults: Solver enabled, Trend none, Weight none",
            Style::default().fg(theme::MUTED),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == selected {
                Style::default().fg(theme::BASE).bg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
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
    let color = theme::SUCCESS;
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
            Style::default().fg(theme::SUBTEXT).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Use optimization solver for WoE binning?",
            Style::default().fg(theme::MUTED),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == current_idx {
                Style::default().fg(theme::BASE).bg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
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
    let color = theme::SUCCESS;
    let options = ["none", "ascending", "descending", "peak", "valley", "auto"];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Monotonicity Constraint",
            Style::default().fg(theme::SUBTEXT).bold(),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == selected {
                Style::default().fg(theme::BASE).bg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
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
    let color = theme::SUCCESS;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SURFACE))
        .title(" Search ")
        .title_style(Style::default().fg(theme::SURFACE));
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search.to_string(), Style::default().fg(theme::TEXT)),
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
                Style::default().fg(theme::BASE).bg(color).bold()
            } else if i == 0 {
                Style::default().fg(theme::MUTED).italic()
            } else {
                Style::default().fg(theme::TEXT)
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
    let color = theme::ERROR;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SURFACE))
        .title(" Search ")
        .title_style(Style::default().fg(theme::SURFACE));
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search.to_string(), Style::default().fg(theme::TEXT)),
        Span::styled("\u{258c}", Style::default().fg(color)),
    ]))
    .block(search_block);
    f.render_widget(search_para, chunks[0]);

    let max_visible = chunks[1].height as usize;
    let start_idx = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, &orig_idx)| {
            let col = &wizard.data.available_columns[orig_idx];
            // Checked state is based on the original column index, not the filtered position
            let is_checked = checked.contains(&orig_idx);
            let checkbox = if is_checked { "[x]" } else { "[ ]" };
            let style = if i == selected {
                Style::default().fg(theme::BASE).bg(color).bold()
            } else if is_checked {
                Style::default().fg(color)
            } else {
                Style::default().fg(theme::TEXT)
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
    let color = theme::SUCCESS;
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
                    Style::default().fg(theme::SUBTEXT).bold(),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Input:        ", Style::default().fg(theme::MUTED)),
                    Span::styled(input, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Target:       ", Style::default().fg(theme::MUTED)),
                    Span::styled(target.to_string(), Style::default().fg(color)),
                ]),
                {
                    if let Some(ref mapping) = wizard.data.target_mapping {
                        Line::from(vec![
                            Span::styled("  Mapping:      ", Style::default().fg(theme::MUTED)),
                            Span::styled(
                                format!(
                                    "event=\"{}\" non-event=\"{}\"",
                                    mapping.event_value, mapping.non_event_value
                                ),
                                Style::default().fg(color),
                            ),
                        ])
                    } else {
                        Line::from("")
                    }
                },
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Missing:      ", Style::default().fg(theme::MUTED)),
                    Span::styled(
                        format!("{:.2}", wizard.data.missing_threshold),
                        Style::default().fg(color),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Gini:         ", Style::default().fg(theme::MUTED)),
                    Span::styled(
                        format!("{:.2}", wizard.data.gini_threshold),
                        Style::default().fg(color),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Correlation:  ", Style::default().fg(theme::MUTED)),
                    Span::styled(
                        format!("{:.2}", wizard.data.correlation_threshold),
                        Style::default().fg(color),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Solver:       ", Style::default().fg(theme::MUTED)),
                    Span::styled(solver.to_string(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Monotonicity: ", Style::default().fg(theme::MUTED)),
                    Span::styled(wizard.data.monotonicity.clone(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Weight:       ", Style::default().fg(theme::MUTED)),
                    Span::styled(weight.to_string(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Drop:         ", Style::default().fg(theme::MUTED)),
                    Span::styled(drop_cols, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Schema:       ", Style::default().fg(theme::MUTED)),
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
                    Style::default().fg(theme::SUBTEXT).bold(),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Input:   ", Style::default().fg(theme::MUTED)),
                    Span::styled(input, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Output:  ", Style::default().fg(theme::MUTED)),
                    Span::styled(output, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Mode:    ", Style::default().fg(theme::MUTED)),
                    Span::styled(mode.to_string(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Schema:  ", Style::default().fg(theme::MUTED)),
                    Span::styled("All rows (full scan)", Style::default().fg(color)),
                ]),
            ]
        }
        Some(WizardTask::Sampling) => {
            let input = wizard
                .data
                .input
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "None".to_string());
            let method = match &wizard.data.sampling_method {
                Some(SamplingMethod::Random) => "Random",
                Some(SamplingMethod::Stratified) => "Stratified",
                Some(SamplingMethod::EqualAllocation) => "Equal allocation",
                None => "None",
            };
            let size_desc = match &wizard.data.sampling_size {
                Some(SampleSize::Count(n)) => format!("{} rows", n),
                Some(SampleSize::Fraction(f)) => format!("{:.1}%", f * 100.0),
                None => "Per-stratum (see below)".to_string(),
            };
            let strata_col = wizard
                .data
                .sampling_strata_column
                .as_deref()
                .unwrap_or("N/A");
            let seed_desc = wizard
                .data
                .sampling_seed
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Random".to_string());

            let mut lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Sampling Summary",
                    Style::default().fg(theme::SUBTEXT).bold(),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Input:    ", Style::default().fg(theme::MUTED)),
                    Span::styled(input, Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Method:   ", Style::default().fg(theme::MUTED)),
                    Span::styled(method.to_string(), Style::default().fg(color)),
                ]),
                Line::from(vec![
                    Span::styled("  Size:     ", Style::default().fg(theme::MUTED)),
                    Span::styled(size_desc, Style::default().fg(color)),
                ]),
            ];

            if wizard.data.sampling_method != Some(SamplingMethod::Random) {
                lines.push(Line::from(vec![
                    Span::styled("  Strata:   ", Style::default().fg(theme::MUTED)),
                    Span::styled(strata_col.to_string(), Style::default().fg(color)),
                ]));
            }

            if wizard.data.sampling_method == Some(SamplingMethod::Stratified)
                && !wizard.data.sampling_strata_specs.is_empty()
            {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  Per-stratum sizes:",
                    Style::default().fg(theme::MUTED),
                )));
                for spec in &wizard.data.sampling_strata_specs {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("    {}: ", spec.value),
                            Style::default().fg(theme::MUTED),
                        ),
                        Span::styled(
                            format!(
                                "{}/{} (weight: {:.2})",
                                spec.sample_size,
                                spec.population_count,
                                if spec.sample_size > 0 {
                                    spec.population_count as f64 / spec.sample_size as f64
                                } else {
                                    0.0
                                }
                            ),
                            Style::default().fg(color),
                        ),
                    ]));
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Seed:     ", Style::default().fg(theme::MUTED)),
                Span::styled(seed_desc, Style::default().fg(color)),
            ]));

            lines
        }
        None => vec![Line::from(Span::styled(
            "Error: No task selected",
            Style::default().fg(theme::ERROR),
        ))],
    };

    let content_height = content.len() as u16;
    let visible_height = area.height;

    // Clamp scroll offset and render scrollable paragraph
    let max_scroll = content_height.saturating_sub(visible_height) as usize;
    let scroll_offset = wizard.summary_scroll.min(max_scroll);

    let paragraph = Paragraph::new(content).scroll((scroll_offset as u16, 0));
    f.render_widget(paragraph, area);

    // Draw scrollbar when content overflows
    if content_height > visible_height {
        let max_scroll_u16 = content_height.saturating_sub(visible_height);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");

        let mut scrollbar_state =
            ScrollbarState::new(max_scroll_u16 as usize).position(scroll_offset);

        // Render scrollbar along the right edge of the summary area
        let scrollbar_area = Rect::new(
            area.x + area.width.saturating_sub(1),
            area.y,
            1,
            area.height,
        );
        f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
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
    let color = theme::PRIMARY;
    let options = output_format_options(wizard.data.input.as_deref());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Output Format",
            Style::default().fg(theme::SUBTEXT).bold(),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == selected {
                Style::default().fg(theme::BASE).bg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
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
    let color = theme::PRIMARY;
    let options = ["Fast (parallel)", "Memory-efficient (streaming)"];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Conversion Mode",
            Style::default().fg(theme::SUBTEXT).bold(),
        )),
    ]);
    f.render_widget(desc, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == selected {
                Style::default().fg(theme::BASE).bg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
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
// Data Loading Helpers
// ============================================================================

/// Load only the target column from a dataset for binary/non-binary analysis.
///
/// Uses Polars lazy evaluation to avoid loading the full dataset. This is used
/// exclusively by the wizard to silently analyze the target column without
/// printing any progress bars or log output.
/// Load only the target column from the dataset for binary/mapping analysis.
///
/// Uses a small row sample (10 000 rows) and minimal schema inference to stay
/// fast even on very large / wide files (e.g. 4 GB, 5 000+ columns).
fn load_target_column_for_analysis(
    path: &std::path::Path,
    target_col: &str,
) -> Result<polars::prelude::DataFrame> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 10 000 rows is more than enough to discover unique target values
    const SAMPLE_ROWS: u32 = 10_000;

    let df = match extension.as_str() {
        "csv" => LazyCsvReader::new(path)
            .with_infer_schema_length(Some(100))
            .with_n_rows(Some(SAMPLE_ROWS as usize))
            .finish()?
            .select([col(target_col)])
            .collect()?,
        "parquet" => LazyFrame::scan_parquet(path, Default::default())?
            .select([col(target_col)])
            .limit(SAMPLE_ROWS)
            .collect()?,
        "sas7bdat" => {
            // SAS7BDAT must load the full file; filter to target column after
            use crate::pipeline::sas7bdat::load_sas7bdat_silent;
            let (full_df, _, _, _) = load_sas7bdat_silent(path)?;
            full_df.select([target_col])?
        }
        _ => anyhow::bail!("Unsupported file format: {}", extension),
    };

    Ok(df)
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
            if wizard.task_selected_index < 2 {
                wizard.task_selected_index += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            wizard.data.task = Some(match wizard.task_selected_index {
                0 => WizardTask::Reduction,
                1 => WizardTask::Conversion,
                _ => WizardTask::Sampling,
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
                let target_col = available_columns[col_index].clone();
                wizard.data.target = Some(target_col.clone());

                // Analyze target to determine if mapping is needed.
                // Load only the target column quietly (no progress bars).
                if let Some(input_path) = &wizard.data.input.clone() {
                    match load_target_column_for_analysis(input_path, &target_col) {
                        Ok(df) => {
                            match crate::pipeline::analyze_target_column(&df, &target_col) {
                                Ok(TargetAnalysis::AlreadyBinary) => {
                                    wizard.data.target_is_binary = true;
                                    wizard.data.target_unique_values = Vec::new();
                                }
                                Ok(TargetAnalysis::NeedsMapping { unique_values }) => {
                                    wizard.data.target_is_binary = false;
                                    wizard.data.target_unique_values = unique_values.clone();
                                    // Populate the TargetMapping step's unique_values
                                    for step in wizard.steps.iter_mut() {
                                        if let WizardStep::TargetMapping {
                                            unique_values: ref mut uv,
                                            ref mut event_selected,
                                            ref mut non_event_selected,
                                            ref mut focus,
                                        } = step
                                        {
                                            *uv = unique_values.clone();
                                            *event_selected = Some(0);
                                            *non_event_selected = Some(0);
                                            *focus = TargetMappingFocus::Event;
                                        }
                                    }
                                }
                                Err(_) => {
                                    // If analysis fails, treat as needing mapping with empty list
                                    wizard.data.target_is_binary = false;
                                    wizard.data.target_unique_values = Vec::new();
                                }
                            }
                        }
                        Err(_) => {
                            // On load failure, fall through — mapping step will show binary message
                            wizard.data.target_is_binary = true;
                            wizard.data.target_unique_values = Vec::new();
                        }
                    }
                }

                Ok(StepAction::NextStep)
            } else {
                Ok(StepAction::Stay)
            }
        }
        _ => Ok(StepAction::Stay),
    }
}

fn handle_target_mapping(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    // Binary target: no mapping needed — Enter continues, Backspace goes back
    if wizard.data.target_is_binary || wizard.data.target_unique_values.is_empty() {
        return match key.code {
            KeyCode::Enter => Ok(StepAction::NextStep),
            KeyCode::Backspace => Ok(StepAction::PrevStep),
            _ => Ok(StepAction::Stay),
        };
    }

    // Non-binary target: two-phase selection (Event then NonEvent)
    let unique_values = wizard.data.target_unique_values.clone();

    let step = wizard.current_step_mut();
    let (step_unique_values, event_selected, non_event_selected, focus) = match step {
        Some(WizardStep::TargetMapping {
            unique_values,
            event_selected,
            non_event_selected,
            focus,
        }) => (unique_values, event_selected, non_event_selected, focus),
        _ => return Ok(StepAction::Stay),
    };

    match focus {
        TargetMappingFocus::Event => match key.code {
            KeyCode::Up => {
                let sel = event_selected.get_or_insert(0);
                if *sel > 0 {
                    *sel -= 1;
                }
                Ok(StepAction::Stay)
            }
            KeyCode::Down => {
                let sel = event_selected.get_or_insert(0);
                if *sel + 1 < unique_values.len() {
                    *sel += 1;
                }
                Ok(StepAction::Stay)
            }
            KeyCode::Enter => {
                if !step_unique_values.is_empty() {
                    event_selected.get_or_insert(0);
                    *non_event_selected = Some(0);
                    *focus = TargetMappingFocus::NonEvent;
                }
                Ok(StepAction::Stay)
            }
            KeyCode::Backspace => Ok(StepAction::PrevStep),
            _ => Ok(StepAction::Stay),
        },
        TargetMappingFocus::NonEvent => match key.code {
            KeyCode::Up => {
                let sel = non_event_selected.get_or_insert(0);
                if *sel > 0 {
                    *sel -= 1;
                }
                Ok(StepAction::Stay)
            }
            KeyCode::Down => {
                let event_idx = event_selected.unwrap_or(0);
                let remaining_len = unique_values.len().saturating_sub(1);
                // Non-event list excludes the event value, adjust index for last item
                let adj_len = if event_idx < unique_values.len() {
                    remaining_len
                } else {
                    unique_values.len()
                };
                let sel = non_event_selected.get_or_insert(0);
                if *sel + 1 < adj_len {
                    *sel += 1;
                }
                Ok(StepAction::Stay)
            }
            KeyCode::Enter => {
                let event_idx = event_selected.unwrap_or(0);
                if event_idx < unique_values.len() {
                    let event_value = unique_values[event_idx].clone();
                    let remaining: Vec<String> = unique_values
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != event_idx)
                        .map(|(_, v)| v.clone())
                        .collect();
                    let non_event_idx = non_event_selected.unwrap_or(0);
                    if non_event_idx < remaining.len() {
                        let non_event_value = remaining[non_event_idx].clone();
                        wizard.data.target_mapping =
                            Some(TargetMapping::new(event_value, non_event_value));
                        return Ok(StepAction::NextStep);
                    }
                }
                Ok(StepAction::Stay)
            }
            KeyCode::Backspace => {
                // Go back to Event selection phase (not prev step)
                *focus = TargetMappingFocus::Event;
                *non_event_selected = None;
                Ok(StepAction::Stay)
            }
            _ => Ok(StepAction::Stay),
        },
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
            // Toggle checkbox by original column index, not filtered position
            if let Some(&orig_idx) = filtered.get(*selected) {
                if checked.contains(&orig_idx) {
                    checked.remove(&orig_idx);
                } else {
                    checked.insert(orig_idx);
                }
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Char(c) => {
            search.push(c);
            // Rebuild filtered list; checked HashSet is preserved untouched
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
            // Rebuild filtered list; checked HashSet is preserved untouched
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
            // Collect checked columns by iterating original indices in order
            wizard.data.columns_to_drop = available_columns
                .iter()
                .enumerate()
                .filter(|(i, _)| checked.contains(i))
                .map(|(_, name)| name.clone())
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

fn handle_summary(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    match key.code {
        KeyCode::Enter => generate_result(wizard),
        KeyCode::Backspace => {
            wizard.summary_scroll = 0;
            Ok(StepAction::PrevStep)
        }
        KeyCode::Up => {
            wizard.summary_scroll = wizard.summary_scroll.saturating_sub(1);
            Ok(StepAction::Stay)
        }
        KeyCode::Down => {
            wizard.summary_scroll = wizard.summary_scroll.saturating_add(1);
            Ok(StepAction::Stay)
        }
        KeyCode::PageUp => {
            wizard.summary_scroll = wizard.summary_scroll.saturating_sub(5);
            Ok(StepAction::Stay)
        }
        KeyCode::PageDown => {
            wizard.summary_scroll = wizard.summary_scroll.saturating_add(5);
            Ok(StepAction::Stay)
        }
        KeyCode::Home => {
            wizard.summary_scroll = 0;
            Ok(StepAction::Stay)
        }
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
// Sampling Step Renderers
// ============================================================================

fn render_sampling_method_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let selected = match wizard.current_step() {
        Some(WizardStep::SamplingMethodSelection { selected }) => *selected,
        _ => return,
    };
    let color = theme::PRIMARY;
    let options = [
        ("Random", "Simple random sample (SRS)"),
        ("Stratified", "Per-stratum sizes (requires strata column)"),
        (
            "Equal allocation",
            "Same n per stratum (requires strata column)",
        ),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Select sampling method",
            Style::default().fg(theme::MUTED),
        )),
    ]);
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (name, desc))| {
            if i == selected {
                let style = Style::default().fg(theme::BASE).bg(color).bold();
                ListItem::new(format!("  {:<20}{}", name, desc)).style(style)
            } else {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {:<20}", name), Style::default().fg(theme::TEXT)),
                    Span::styled(*desc, Style::default().fg(theme::MUTED)),
                ]))
            }
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_sample_size_input(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error, is_count) = match wizard.current_step() {
        Some(WizardStep::SampleSizeInput {
            input,
            error,
            is_count,
        }) => (input, error, *is_count),
        _ => return,
    };

    let mode_label = if is_count { "Count" } else { "Fraction" };
    let description = if is_count {
        "Enter the number of rows to sample"
    } else {
        "Enter fraction (0.0-1.0) of rows to sample"
    };

    let title = format!("Sample Size ({})", mode_label);
    render_threshold_content(f, area, &title, description, input, error);

    // Tab hint at the bottom
    let hint_y = area.y + 8;
    if hint_y < area.y + area.height {
        let hint_area = Rect::new(area.x, hint_y, area.width, 1);
        let toggle = if is_count {
            "Tab: switch to fraction"
        } else {
            "Tab: switch to count"
        };
        f.render_widget(
            Paragraph::new(Span::styled(toggle, Style::default().fg(theme::MUTED))),
            hint_area,
        );
    }
}

fn render_strata_column_selection(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (search, filtered, selected) = match wizard.current_step() {
        Some(WizardStep::StrataColumnSelection {
            search,
            filtered,
            selected,
        }) => (search, filtered, *selected),
        _ => return,
    };

    let color = theme::ACCENT;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Search box
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SURFACE))
        .title(" Search ")
        .title_style(Style::default().fg(theme::SURFACE));

    let search_para = Paragraph::new(Line::from(vec![
        Span::styled(search.to_string(), Style::default().fg(theme::TEXT)),
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
                Style::default().fg(theme::BASE).bg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
            };
            ListItem::new(format!("  {}", col)).style(style)
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_stratum_size_config(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (strata, selected, error, scroll_offset) = match wizard.current_step() {
        Some(WizardStep::StratumSizeConfig {
            strata,
            selected,
            error,
            scroll_offset,
        }) => (strata, *selected, error, *scroll_offset),
        _ => return,
    };

    let color = theme::WARNING;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Header
            Constraint::Min(1),    // Table
            Constraint::Length(2), // Footer (total + error)
        ])
        .split(area);

    // Header
    let header = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Configure sample sizes per stratum",
            Style::default().fg(theme::SUBTEXT).bold(),
        )),
        Line::from(vec![
            Span::styled("  Stratum", Style::default().fg(theme::MUTED)),
            Span::styled("                    N_h", Style::default().fg(theme::MUTED)),
            Span::styled("       n_h", Style::default().fg(theme::MUTED)),
        ]),
    ]);
    f.render_widget(header, chunks[0]);

    // Table rows
    let max_visible = chunks[1].height as usize;
    let start_idx = scroll_offset;

    let items: Vec<ListItem> = strata
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(max_visible)
        .map(|(i, (value, n_h, input_str))| {
            let is_selected = i == selected;
            let prefix = if is_selected { "> " } else { "  " };
            let value_display = if value.len() > 18 {
                format!("{}...", &value[..15])
            } else {
                format!("{:<18}", value)
            };
            let n_h_display = format!("{:>8}", format_number(*n_h));
            let input_display = if is_selected {
                format!("  [{}{}]", input_str, "\u{258c}")
            } else if input_str.is_empty() {
                "  [___]".to_string()
            } else {
                format!("  {}", input_str)
            };

            let style = if is_selected {
                Style::default().fg(color).bold()
            } else {
                Style::default().fg(theme::TEXT)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{}{}", prefix, value_display), style),
                Span::styled(n_h_display, Style::default().fg(theme::MUTED)),
                Span::styled(input_display, style),
            ]))
        })
        .collect();

    let list = List::new(items);
    let mut list_state = ListState::default();
    list_state.select(Some(selected.saturating_sub(start_idx)));
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    // Footer: total + error
    let total_n: usize = strata
        .iter()
        .filter_map(|(_, _, s)| s.parse::<usize>().ok())
        .sum();
    let total_pop: usize = strata.iter().map(|(_, n, _)| n).sum();
    let mut footer_lines = vec![Line::from(vec![
        Span::styled(
            format!("  Total: {}", format_number(total_pop)),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(
            format!("                  {}", format_number(total_n)),
            Style::default().fg(color),
        ),
    ])];
    if let Some(err) = error {
        footer_lines.push(Line::from(Span::styled(
            format!("  {}", err),
            Style::default().fg(theme::ERROR),
        )));
    }
    f.render_widget(Paragraph::new(footer_lines), chunks[2]);
}

fn render_seed_input(f: &mut Frame, area: Rect, wizard: &WizardState) {
    let (input, error) = match wizard.current_step() {
        Some(WizardStep::SeedInput { input, error }) => (input, error),
        _ => return,
    };

    render_threshold_content(
        f,
        area,
        "Random Seed (optional)",
        "Enter a seed for reproducibility, or leave empty for random",
        input,
        error,
    );
}

/// Format a number with thousands separators
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

// ============================================================================
// Sampling Step Handlers
// ============================================================================

fn handle_sampling_method_selection(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let selected = match step {
        Some(WizardStep::SamplingMethodSelection { selected }) => selected,
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
            if *selected < 2 {
                *selected += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Enter => {
            let method = match *selected {
                0 => SamplingMethod::Random,
                1 => SamplingMethod::Stratified,
                _ => SamplingMethod::EqualAllocation,
            };
            wizard.data.sampling_method = Some(method);
            wizard.build_steps();
            // Move to step after SamplingMethodSelection (index 2)
            wizard.current_index = 2;
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => Ok(StepAction::PrevStep),
        _ => Ok(StepAction::Stay),
    }
}

fn handle_sample_size_input(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let (input, error, is_count) = match step {
        Some(WizardStep::SampleSizeInput {
            input,
            error,
            is_count,
        }) => (input, error, is_count),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            input.push(c);
            *error = None;
            Ok(StepAction::Stay)
        }
        KeyCode::Tab => {
            *is_count = !*is_count;
            input.clear();
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
        KeyCode::Enter => {
            if input.is_empty() {
                *error = Some("Value is required".to_string());
                return Ok(StepAction::Stay);
            }
            if *is_count {
                match input.parse::<usize>() {
                    Ok(0) => {
                        *error = Some("Sample size must be positive".to_string());
                        Ok(StepAction::Stay)
                    }
                    Ok(n) => {
                        wizard.data.sampling_size = Some(SampleSize::Count(n));
                        Ok(StepAction::NextStep)
                    }
                    Err(_) => {
                        *error = Some("Invalid number".to_string());
                        Ok(StepAction::Stay)
                    }
                }
            } else {
                match input.parse::<f64>() {
                    Ok(f) if f <= 0.0 => {
                        *error = Some("Fraction must be positive".to_string());
                        Ok(StepAction::Stay)
                    }
                    Ok(f) if f >= 1.0 => {
                        *error = Some("Fraction must be in (0.0, 1.0)".to_string());
                        Ok(StepAction::Stay)
                    }
                    Ok(f) => {
                        wizard.data.sampling_size = Some(SampleSize::Fraction(f));
                        Ok(StepAction::NextStep)
                    }
                    Err(_) => {
                        *error = Some("Invalid number".to_string());
                        Ok(StepAction::Stay)
                    }
                }
            }
        }
        _ => Ok(StepAction::Stay),
    }
}

fn handle_strata_column_selection(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let available_columns = wizard.data.available_columns.clone();

    let step = wizard.current_step_mut();
    let (search, filtered, selected) = match step {
        Some(WizardStep::StrataColumnSelection {
            search,
            filtered,
            selected,
        }) => (search, filtered, selected),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Char(c) => {
            search.push(c);
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
                let col_name = available_columns[col_index].clone();
                wizard.data.sampling_strata_column = Some(col_name.clone());

                // Load strata info from the dataset
                if let Some(input_path) = &wizard.data.input.clone() {
                    let df = load_target_column_for_analysis(input_path, &col_name)?;
                    let strata_info = crate::pipeline::analyze_strata(&df, &col_name)?;
                    wizard.data.sampling_strata_info = strata_info;
                }

                // Rebuild steps to include StratumSizeConfig if stratified
                wizard.build_steps();

                // Navigate to the step after StrataColumnSelection (index 3)
                wizard.current_index = 3;
                Ok(StepAction::Stay)
            } else {
                Ok(StepAction::Stay)
            }
        }
        _ => Ok(StepAction::Stay),
    }
}

fn handle_stratum_size_config(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let (strata, selected, error, scroll_offset) = match step {
        Some(WizardStep::StratumSizeConfig {
            strata,
            selected,
            error,
            scroll_offset,
        }) => (strata, selected, error, scroll_offset),
        _ => return Ok(StepAction::Stay),
    };

    match key.code {
        KeyCode::Up => {
            if *selected > 0 {
                *selected -= 1;
                if *selected < *scroll_offset {
                    *scroll_offset = *selected;
                }
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Down => {
            if *selected < strata.len().saturating_sub(1) {
                *selected += 1;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if let Some(row) = strata.get_mut(*selected) {
                row.2.push(c);
                *error = None;
            }
            Ok(StepAction::Stay)
        }
        KeyCode::Backspace => {
            if let Some(row) = strata.get_mut(*selected) {
                if !row.2.is_empty() {
                    row.2.pop();
                    *error = None;
                    return Ok(StepAction::Stay);
                }
            }
            Ok(StepAction::PrevStep)
        }
        KeyCode::Enter => {
            // Validate all strata have valid sizes
            let mut specs = Vec::new();
            for (value, pop_count, input_str) in strata.iter() {
                if input_str.is_empty() {
                    *error = Some(format!("Missing size for stratum '{}'", value));
                    return Ok(StepAction::Stay);
                }
                match input_str.parse::<usize>() {
                    Ok(0) => {
                        *error = Some(format!("Size for '{}' must be positive", value));
                        return Ok(StepAction::Stay);
                    }
                    Ok(n) if n > *pop_count => {
                        *error = Some(format!(
                            "Size {} exceeds population {} for '{}'",
                            n, pop_count, value
                        ));
                        return Ok(StepAction::Stay);
                    }
                    Ok(n) => {
                        specs.push(StratumSpec {
                            value: value.clone(),
                            population_count: *pop_count,
                            sample_size: n,
                        });
                    }
                    Err(_) => {
                        *error = Some(format!("Invalid number for '{}'", value));
                        return Ok(StepAction::Stay);
                    }
                }
            }
            wizard.data.sampling_strata_specs = specs;
            wizard.data.sampling_size = None; // Stratified uses per-stratum specs
            Ok(StepAction::NextStep)
        }
        _ => Ok(StepAction::Stay),
    }
}

fn handle_seed_input(wizard: &mut WizardState, key: KeyEvent) -> Result<StepAction> {
    let step = wizard.current_step_mut();
    let (input, error) = match step {
        Some(WizardStep::SeedInput { input, error }) => (input, error),
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
        KeyCode::Enter => {
            if input.is_empty() {
                // Empty = no seed (random)
                wizard.data.sampling_seed = None;
            } else {
                match input.parse::<u64>() {
                    Ok(seed) => {
                        wizard.data.sampling_seed = Some(seed);
                    }
                    Err(_) => {
                        *error = Some("Invalid seed number".to_string());
                        return Ok(StepAction::Stay);
                    }
                }
            }
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
