# Implementation Plan: TUI Wizard Mode

**Spec Version:** 0.1.0
**Date:** 2026-02-02
**Branch:** `3-tui-wizard`

---

## Constitution Check

### 1. Statistical Correctness
**Status:** Not Applicable
**Rationale:** This feature is purely UI/UX focused. No changes to statistical analysis logic (missing values, Gini/IV, correlation calculations). The wizard collects the same configuration parameters that are currently available through CLI/dashboard.

### 2. Performance at Scale
**Status:** Partially Applicable
**Rationale:** Wizard runs during configuration phase before data processing. However, P2 mandates "the TUI MUST offer CSV-to-Parquet conversion as a convenience," which is directly addressed by the conversion wizard path (Phase 3). NFR-2 (transitions < 100ms, loading indicator if > 200ms) also addresses UI responsiveness. The only performance consideration is lightweight column name loading during file selection (already implemented via `get_column_names()`), which uses Polars schema inference and is fast even for large files.

### 3. Transparent Decision-Making
**Status:** Applicable and Addressed
**Rationale:** The wizard must clearly communicate what each configuration option controls. Each wizard step will include:
- Clear step title (e.g., "Configure Missing Value Threshold")
- Explanatory text describing the parameter's purpose
- Default value with rationale
- Help text showing valid ranges and impact
- Summary screen showing all selections before proceeding

### 4. Ergonomic TUI/CLI
**Status:** Primary Principle - Core Focus
**Rationale:** This is the main motivation for the feature. The wizard addresses user feedback that the dashboard is overwhelming for first-time users. Key improvements:
- Linear guided flow vs. all-at-once dashboard
- Progressive disclosure (only show relevant options)
- Visual progress indicator (Step X of Y)
- `--manual` flag preserves expert workflow
- Wizard defaults to sensible values, reducing decision fatigue

### 5. Rigorous Testing
**Status:** Applicable and Addressed
**Rationale:** Wizard state machine requires thorough testing:
- Unit tests for validation logic (threshold ranges, file paths)
- Unit tests for state transitions (step progression, branching)
- CLI integration tests for `--manual` flag behavior
- Manual TUI testing for visual/interaction validation

---

## Architecture Overview

### High-Level Approach

The wizard is implemented as a **separate, parallel TUI module** (`src/cli/wizard.rs`) that runs **before** the existing dashboard. The architecture follows these principles:

1. **Separation of Concerns:** Wizard lives in its own module, keeping `config_menu.rs` unchanged for expert users
2. **State Machine Pattern:** Indexed step approach with `Vec<WizardStep>` and `current_index: usize`
3. **Accumulated State:** `WizardData` struct with `Option<T>` fields that are populated as user progresses
4. **Branching Logic:** Task selection step determines reduction vs. conversion path
5. **Integration Point:** `setup_configuration()` in `main.rs` becomes the orchestrator with three paths:
   - `--no-confirm` → CLI-only (existing)
   - `--manual` → Dashboard (existing)
   - Default → Wizard (new)

### Data Flow

```
main.rs
  └─> setup_configuration()
       ├─> --no-confirm? → Build PipelineConfig from CLI args directly
       ├─> --manual? → run_config_menu() → PipelineConfig
       └─> Default → run_wizard() → WizardResult
                       ├─> WizardTask::Reduction → Reduction wizard path → Config → PipelineConfig
                       └─> WizardTask::Conversion → Conversion wizard path → ConversionConfig
```

### Key Types

```rust
// src/cli/wizard.rs

/// The result of running the wizard
pub enum WizardResult {
    /// User completed wizard and chose to proceed with reduction
    RunReduction(Box<Config>),
    /// User completed wizard and chose to convert CSV to Parquet
    RunConversion(Box<ConversionConfig>),
    /// User quit the wizard
    Quit,
}

/// Conversion-specific config (Config struct lacks conversion fields)
pub struct ConversionConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub infer_schema_length: usize,
    pub fast: bool,
}

/// Task selection at the beginning
#[derive(Debug, Clone, PartialEq)]
pub enum WizardTask {
    Reduction,
    Conversion,
}

/// Accumulated wizard state (fields populated progressively)
#[derive(Debug, Clone)]
pub struct WizardData {
    // Task context
    pub task: Option<WizardTask>,
    pub input_path: Option<PathBuf>,
    pub columns: Vec<String>,

    // Reduction parameters (bare f64 with defaults)
    pub target: Option<String>,
    pub missing_threshold: f64,       // Default: 0.30
    pub gini_threshold: f64,          // Default: 0.05
    pub correlation_threshold: f64,   // Default: 0.40
    pub use_solver: bool,             // Default: true
    pub monotonicity: String,         // Default: "none"
    pub weight_column: Option<String>,
    pub drop_columns: Vec<String>,
    pub infer_schema_length: usize,   // Default: 10000

    // Conversion parameters
    pub output_path: Option<PathBuf>,
    pub conversion_fast: bool,        // Default: true

    // Target mapping (non-binary targets)
    pub target_mapping: Option<TargetMapping>,
}

/// Individual wizard step (see data-model.md for full variants with state)
#[derive(Debug, Clone, PartialEq)]
pub enum WizardStep {
    // Shared steps
    TaskSelection,
    FileSelection,

    // Reduction path steps
    TargetSelection,
    TargetMapping,          // Non-binary target event/non-event selection
    MissingThreshold,       // One threshold per step (per FR-6)
    GiniThreshold,
    CorrelationThreshold,
    OptionalSettingsPrompt,
    SolverToggle,
    MonotonicitySelection,
    WeightColumn,
    DropColumns,
    SchemaInference,
    Summary,

    // Conversion path steps
    OutputPath,
    ConversionMode,
}

> **Note:** This plan shows `WizardStep` as a simple enum for clarity. The authoritative type definition is in `data-model.md`, where each variant carries embedded UI state (search strings, filtered indices, error messages). The `WizardData` struct accumulates the final configuration values, while step-specific transient UI state lives in the enum variants.

/// Main wizard state
pub struct WizardState {
    pub steps: Vec<WizardStep>,
    pub current_index: usize,
    pub data: WizardData,
    pub show_quit_confirm: bool,
}
```

### Integration Points

1. **`src/main.rs:setup_configuration()`** - Add wizard path (lines 231-329)
2. **`src/cli/mod.rs`** - Export wizard module
3. **`src/cli/args.rs`** - Add `--manual` flag
4. **`src/cli/wizard.rs`** - New module (primary implementation)

---

## Implementation Steps

### Phase 1: Core Wizard Infrastructure

**Goal:** Create the wizard module skeleton with state machine, terminal management, and basic rendering.

#### Step 1.1: Create Wizard Module Structure

**File:** `src/cli/wizard.rs`

**Tasks:**
- Define `WizardResult`, `WizardTask`, `WizardData`, `WizardStep`, `WizardState` enums/structs (as shown above)
- Implement `WizardData::default()` with sensible defaults (bare f64 fields: 0.30, 0.05, 0.40; bool fields: true/false)
- Implement `WizardState::new()` that initializes with empty step vector and default data
- Add module-level documentation explaining the wizard's purpose

**Acceptance Criteria:**
- [ ] All types compile without errors
- [ ] `WizardData` has sensible defaults (e.g., `missing_threshold: 0.30` as bare f64, not Option)
- [ ] WizardState struct has clear field documentation

#### Step 1.2: Implement Step Sequencing Logic

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `WizardState::build_steps()` method that populates `steps` based on task selection:
  ```rust
  fn build_steps(&mut self) {
      self.steps.clear();
      self.steps.push(WizardStep::TaskSelection);
      self.steps.push(WizardStep::FileSelection);

      match self.data.task {
          Some(WizardTask::Reduction) => {
              self.steps.push(WizardStep::TargetSelection);
              self.steps.push(WizardStep::TargetMapping);  // Always shown; pre-selects for binary targets
              self.steps.push(WizardStep::MissingThreshold);
              self.steps.push(WizardStep::GiniThreshold);
              self.steps.push(WizardStep::CorrelationThreshold);
              self.steps.push(WizardStep::OptionalSettingsPrompt);
              self.steps.push(WizardStep::Summary);
          }
          Some(WizardTask::Conversion) => {
              self.steps.push(WizardStep::OutputPath);
              self.steps.push(WizardStep::ConversionMode);
              self.steps.push(WizardStep::Summary);
          }
          None => {
              // Only initial steps before task selection
          }
      }
  }
  ```
- Implement `WizardState::next_step()` → increments `current_index`, returns `Result<()>` (error if out of bounds)
- Implement `WizardState::prev_step()` → decrements `current_index`, returns `Result<()>` (error if index == 0)
- Implement `WizardState::current_step()` → returns `Option<&WizardStep>`
- Implement `WizardState::is_last_step()` → returns `bool`

**Acceptance Criteria:**
- [ ] Step navigation works correctly in both directions
- [ ] `build_steps()` correctly branches based on task selection
- [ ] Attempting to go before first step or after last step returns `Err`

#### Step 1.3: Terminal Setup and Teardown

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>>`:
  ```rust
  fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
      enable_raw_mode()?;
      stdout().execute(EnterAlternateScreen)?;
      let backend = CrosstermBackend::new(stdout());
      let terminal = Terminal::new(backend)?;
      Ok(terminal)
  }
  ```
- Implement `teardown_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()>`:
  ```rust
  fn teardown_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
      disable_raw_mode()?;
      stdout().execute(LeaveAlternateScreen)?;
      terminal.show_cursor()?;
      Ok(())
  }
  ```
- Add error handling to ensure teardown runs even on panic (using `std::panic::catch_unwind` if needed)

**Acceptance Criteria:**
- [ ] Terminal enters alternate screen and raw mode correctly
- [ ] Terminal cleanup happens even if wizard panics or returns early
- [ ] Cursor is restored after wizard exits

#### Step 1.4: Main Wizard Entry Point

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement public `run_wizard(cli: &Cli) -> Result<WizardResult>`:
  ```rust
  pub fn run_wizard(cli: &Cli) -> Result<WizardResult> {
      let mut terminal = setup_terminal()?;
      let mut wizard = WizardState::new();

      // Pre-populate from CLI args if provided
      if let Some(input) = &cli.input {
          wizard.data.input_path = Some(input.clone());
      }
      if let Some(target) = &cli.target {
          wizard.data.target = Some(target.clone());
      }
      // ... populate other CLI args (thresholds, solver, etc.)

      wizard.build_steps();  // Build initial steps

      let result = run_wizard_loop(&mut terminal, &mut wizard);

      teardown_terminal(&mut terminal)?;

      result
  }
  ```
- Implement `run_wizard_loop(terminal, wizard) -> Result<WizardResult>` (event loop placeholder for now)

**Acceptance Criteria:**
- [ ] `run_wizard()` compiles and can be called from `main.rs`
- [ ] CLI arguments are pre-populated into `WizardData` when provided
- [ ] Terminal setup/teardown is guaranteed even on early return

#### Step 1.5: Basic Rendering Framework

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_wizard(frame: &mut Frame, wizard: &WizardState)`:
  - Use `Layout::default().direction(Direction::Vertical)` to split into:
    - Top bar (height 3) - Progress indicator
    - Main content (remaining space) - Step-specific content
    - Help bar (height 3) - Keyboard shortcuts
  - Render top bar with: `"Step X of Y: [Step Title]"`
  - Render help bar with common shortcuts: `[Enter] Continue  [Esc] Back  [Q] Quit`
- Implement `get_step_title(step: &WizardStep) -> &'static str` for each step
- Implement placeholder `render_step(frame: &mut Frame, area: Rect, wizard: &WizardState)` that just shows step name

**Acceptance Criteria:**
- [ ] Top bar correctly shows current step number and total
- [ ] Help bar shows contextual keyboard shortcuts
- [ ] Main content area renders placeholder for any step

---

### Phase 2: Reduction Path Steps

**Goal:** Implement each wizard step for the reduction pipeline, with validation and state updates.

#### Step 2.1: Task Selection Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_task_selection(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "What would you like to do?"
  - Show two options (list with highlight on selected):
    - "Reduce features in a dataset" (with description: "Analyze and drop features based on missing values, Gini, correlation")
    - "Convert CSV to Parquet" (with description: "Fast conversion to columnar format")
  - Arrow keys to navigate, Enter to select
- Implement `handle_task_selection_event(event: KeyEvent, wizard: &mut WizardState) -> Result<StepAction>`:
  - Up/Down: change selection
  - Enter: set `wizard.data.task`, call `wizard.build_steps()`, return `StepAction::NextStep`
  - Escape: show quit confirmation dialog (return `StepAction::ShowQuitConfirm`)

**Acceptance Criteria:**
- [ ] Task options are clearly described
- [ ] Selection is visually distinct (highlight color)
- [ ] Selecting a task rebuilds the step sequence correctly
- [ ] Wizard advances to file selection after task is chosen

#### Step 2.2: File Selection Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- **Reuse existing file selector:** Call `run_file_selector()` from `config_menu.rs`
- If `wizard.data.input_path` is already populated (from CLI), skip interactive selection and use that value
- After file is selected:
  - Set `wizard.data.input_path`
  - Call `get_column_names(&input_path)` and store in `wizard.data.columns`
  - Derive default output path and set `wizard.data.output_path`
  - Return `StepAction::NextStep`

**Note:** This step uses the existing `run_file_selector()` function, so no new rendering logic needed. Just integrate it into the wizard flow.

**Acceptance Criteria:**
- [ ] File selector launches correctly from wizard
- [ ] Selected file path is stored in `WizardData`
- [ ] Column names are loaded and available for subsequent steps
- [ ] Default output path is derived correctly

#### Step 2.3: Target Selection Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_target_selection(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "Select Target Column"
  - Description: "The target column is preserved during reduction and used for Gini/IV analysis"
  - Show searchable list of columns (reuse search UI pattern from `config_menu.rs`)
  - Display current selection if `wizard.data.target.is_some()`
- Implement `handle_target_selection_event(event: KeyEvent, wizard: &mut WizardState, state: &mut SelectionState) -> Result<StepAction>`:
  - Search input (typing filters list)
  - Up/Down: navigate filtered list
  - Enter: set `wizard.data.target`, return `StepAction::NextStep`
  - Backspace (empty search): return `StepAction::PrevStep`
  - Escape: show quit confirmation dialog (return `StepAction::ShowQuitConfirm`)
- Add validation: Target column must exist in `wizard.columns`

**Acceptance Criteria:**
- [ ] Column list is searchable and filterable
- [ ] Selected target is highlighted and stored correctly
- [ ] Cannot proceed without selecting a target
- [ ] Backspace returns to previous step

#### Step 2.4: Missing Threshold Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_missing_threshold(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "Configure Missing Value Threshold"
  - Display editable field with default value `0.30`
  - Show help text: "Features with more than this proportion of missing values will be dropped. Default: 0.30 (30%)"
  - Show current input and inline error if invalid
- Implement `handle_missing_threshold_event(event: KeyEvent, wizard: &mut WizardState) -> Result<StepAction>`:
  - Typing: edit value
  - Enter: validate (0.0-1.0), store in `wizard.data.missing_threshold`, return `StepAction::NextStep`
  - Escape: show quit confirmation dialog (return `StepAction::ShowQuitConfirm`)

**Acceptance Criteria:**
- [ ] Default value (0.30) is pre-populated
- [ ] Invalid input shows inline error: "Threshold must be between 0.0 and 1.0"
- [ ] Valid threshold is stored and wizard advances to Gini threshold

#### Step 2.4b: Gini Threshold Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_gini_threshold(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "Configure Gini/IV Threshold"
  - Display editable field with default value `0.05`
  - Show help text: "Features with a Gini coefficient below this value will be dropped. Default: 0.05"
- Implement `handle_gini_threshold_event(event: KeyEvent, wizard: &mut WizardState) -> Result<StepAction>`:
  - Same pattern as missing threshold

**Acceptance Criteria:**
- [ ] Default value (0.05) is pre-populated
- [ ] Validation and error display work identically to missing threshold

#### Step 2.4c: Correlation Threshold Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_correlation_threshold(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "Configure Correlation Threshold"
  - Display editable field with default value `0.40`
  - Show help text: "One feature from each pair with correlation above this value will be dropped. Default: 0.40"
- Implement `handle_correlation_threshold_event(event: KeyEvent, wizard: &mut WizardState) -> Result<StepAction>`:
  - Same pattern as missing threshold

**Acceptance Criteria:**
- [ ] Default value (0.40) is pre-populated
- [ ] Validation and error display work identically to missing threshold

#### Step 2.5: Optional Configuration Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_optional_config(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "Optional Settings"
  - Show menu of configurable options:
    ```
    [S] Solver Options        Current: Enabled, Trend: none
    [W] Weight Column         Current: None
    [D] Drop Columns          Current: None
    [A] Advanced (Schema)     Current: 10000 rows

    [Enter] Continue with these settings
    ```
  - Each option opens a sub-dialog (similar to dashboard behavior)
- Implement `handle_optional_config_event(event: KeyEvent, wizard: &mut WizardState) -> Result<StepAction>`:
  - S: Open solver options dialog (toggle + monotonicity selection)
  - W: Open weight column selector (searchable list)
  - D: Open multi-select column dropper (checkboxes)
  - A: Open schema inference input (numeric input)
  - Enter: return `StepAction::NextStep`
  - Backspace: return `StepAction::PrevStep`
  - Escape: show quit confirmation dialog (return `StepAction::ShowQuitConfirm`)
- Reuse existing UI components from `config_menu.rs` where possible

**Acceptance Criteria:**
- [ ] All optional settings are accessible
- [ ] Each sub-dialog stores updated values in `WizardData`
- [ ] User can skip this step entirely (press Enter immediately)
- [ ] Current values are displayed next to each option

#### Step 2.6: Reduction Summary Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_reduction_summary(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "Summary - Ready to Proceed"
  - Display all collected settings in a formatted table:
    ```
    Configuration Summary
    ═════════════════════════════════════════
    Task:               Feature Reduction
    Input File:         /path/to/data.csv
    Target Column:      target_variable
    Output File:        /path/to/data_reduced.csv

    Thresholds:
      Missing:          0.30
      Gini:             0.05
      Correlation:      0.40

    Solver:             Enabled (Trend: none)
    Weight Column:      None
    Columns to Drop:    None
    Schema Inference:   10000 rows

    [Enter] Start Pipeline  [Backspace] Go Back  [Esc] Cancel
    ```
- Implement `handle_reduction_summary_event(event: KeyEvent, wizard: &WizardState) -> Result<StepAction>`:
  - Enter: Convert `wizard.data` to `Config`, return `StepAction::Complete(WizardResult::RunReduction(config))`
  - Backspace: return `StepAction::PrevStep`
  - Escape: show quit confirmation dialog (return `StepAction::ShowQuitConfirm`)

**Acceptance Criteria:**
- [ ] All settings are displayed clearly
- [ ] Missing required fields show error (should not reach this step if incomplete)
- [ ] Enter key builds valid `Config` struct
- [ ] Config matches the format expected by `setup_configuration()`

---

### Phase 3: Conversion Path Steps

**Goal:** Implement wizard steps for CSV-to-Parquet conversion.

#### Step 3.1: Conversion Output Path Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_conversion_output(frame: &mut Frame, area: Rect, wizard: &WizardState, input: &str)`:
  - Show title: "Choose Output Path"
  - Display input file path
  - Show editable field with default output path (input with `.parquet` extension)
  - Show help: "Edit path or press Enter to use default"
- Implement `handle_conversion_output_event(event: KeyEvent, wizard: &mut WizardState, input: &mut String) -> Result<StepAction>`:
  - Typing: edit output path
  - Enter: validate path (must be `.parquet` extension), set `wizard.data.output_path`, return `StepAction::NextStep`
  - Backspace (empty field): return `StepAction::PrevStep`
  - Escape: show quit confirmation dialog (return `StepAction::ShowQuitConfirm`)

**Acceptance Criteria:**
- [ ] Default output path is correctly derived (same dir, `.parquet` extension)
- [ ] User can edit output path
- [ ] Validation ensures `.parquet` extension
- [ ] Invalid paths show error message

#### Step 3.2: Conversion Mode Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_conversion_mode(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "Select Conversion Mode"
  - Show two options:
    - "Fast (uses more RAM, parallelizes across all CPUs)" [Recommended for machines with enough RAM]
    - "Memory-efficient (streaming, single-threaded, low RAM)"
  - Show description of trade-offs
- Implement `handle_conversion_mode_event(event: KeyEvent, wizard: &mut WizardState) -> Result<StepAction>`:
  - Up/Down: change selection
  - Enter: set `wizard.data.conversion_fast`, return `StepAction::NextStep`
  - Escape: show quit confirmation dialog (return `StepAction::ShowQuitConfirm`)

**Acceptance Criteria:**
- [ ] Two modes are clearly explained
- [ ] Selection is visually distinct
- [ ] `conversion_fast` boolean is set correctly (true for fast, false for streaming)

#### Step 3.3: Conversion Summary Step

**File:** `src/cli/wizard.rs`

**Tasks:**
- Implement `render_conversion_summary(frame: &mut Frame, area: Rect, wizard: &WizardState)`:
  - Show title: "Summary - Ready to Convert"
  - Display settings:
    ```
    Configuration Summary
    ═════════════════════════════════════════
    Task:               CSV to Parquet Conversion
    Input File:         /path/to/data.csv
    Output File:        /path/to/data.parquet
    Mode:               Fast (parallel)
    Schema Inference:   10000 rows

    [Enter] Start Conversion  [Backspace] Go Back  [Esc] Cancel
    ```
- Implement `handle_conversion_summary_event(event: KeyEvent, wizard: &WizardState) -> Result<StepAction>`:
  - Enter: Build `WizardResult::RunConversion`, return `StepAction::Complete`
  - Backspace: return `StepAction::PrevStep`
  - Escape: show quit confirmation dialog (return `StepAction::ShowQuitConfirm`)

**Acceptance Criteria:**
- [ ] All conversion settings are displayed
- [ ] Enter key returns correct `WizardResult::RunConversion` with all fields populated
- [ ] Result can be used directly by `cli::convert::run_convert()`

---

### Phase 4: CLI Integration

**Goal:** Add `--manual` flag and integrate wizard into main.rs orchestration.

#### Step 4.1: Add --manual Flag

**File:** `src/cli/args.rs`

**Tasks:**
- Add field to `Cli` struct:
  ```rust
  /// Skip wizard and go directly to manual configuration dashboard.
  /// For expert users who prefer full control over all settings.
  #[arg(long, default_value = "false")]
  pub manual: bool,
  ```
- Update help text in `Cli` struct documentation

**Acceptance Criteria:**
- [ ] `--manual` flag is recognized by clap
- [ ] Help text clearly explains the flag's purpose
- [ ] Default is `false` (wizard runs by default)

#### Step 4.2: Export Wizard Module

**File:** `src/cli/mod.rs`

**Tasks:**
- Add `pub mod wizard;` after existing module declarations
- Add `pub use wizard::{run_wizard, WizardResult};` to public exports

**Acceptance Criteria:**
- [ ] Wizard module compiles without errors
- [ ] `run_wizard` and `WizardResult` are accessible from `main.rs`

#### Step 4.3: Update setup_configuration() Orchestration

**File:** `src/main.rs`

**Tasks:**
- Replace existing `setup_configuration()` function (lines 231-350) with new three-way branching logic:
  ```rust
  fn setup_configuration(
      cli: &Cli,
      input: &std::path::Path,
      output_path: &std::path::Path,
  ) -> Result<PipelineConfig> {
      // Build CLI target mapping
      let cli_target_mapping = match (&cli.event_value, &cli.non_event_value) {
          (Some(event), Some(non_event)) => {
              Some(TargetMapping::new(event.clone(), non_event.clone()))
          }
          (Some(_), None) | (None, Some(_)) => {
              anyhow::bail!("Both --event-value and --non-event-value must be provided together")
          }
          (None, None) => None,
      };

      // Branch 1: --no-confirm (CLI-only, existing behavior)
      if cli.no_confirm {
          let target = cli.target.clone().ok_or_else(|| {
              anyhow::anyhow!("Target column is required when using --no-confirm. Use -t/--target to specify.")
          })?;

          return Ok(PipelineConfig {
              target,
              missing_threshold: cli.missing_threshold,
              gini_threshold: cli.gini_threshold,
              gini_bins: cli.gini_bins,
              correlation_threshold: cli.correlation_threshold,
              columns_to_drop: cli.drop_columns.clone(),
              target_mapping: cli_target_mapping,
              weight_column: cli.weight_column.clone(),
              binning_strategy: cli.binning_strategy.clone(),
              prebins: cli.prebins,
              cart_min_bin_pct: cli.cart_min_bin_pct,
              min_category_samples: cli.min_category_samples,
              use_solver: cli.use_solver,
              monotonicity: cli.monotonicity.clone(),
              solver_timeout: cli.solver_timeout,
              solver_gap: cli.solver_gap,
              infer_schema_length: cli.infer_schema_length,
          });
      }

      // Branch 2: --manual (Dashboard, existing behavior)
      if cli.manual {
          let mut current_input = input.to_path_buf();
          let mut columns = get_column_names(&current_input)?;

          let mut config = Config {
              input: current_input.clone(),
              target: cli.target.clone(),
              output: output_path.to_path_buf(),
              missing_threshold: cli.missing_threshold,
              gini_threshold: cli.gini_threshold,
              correlation_threshold: cli.correlation_threshold,
              columns_to_drop: cli.drop_columns.clone(),
              target_mapping: cli_target_mapping.clone(),
              weight_column: cli.weight_column.clone(),
              binning_strategy: cli.binning_strategy.clone(),
              gini_bins: cli.gini_bins,
              prebins: cli.prebins,
              cart_min_bin_pct: cli.cart_min_bin_pct,
              min_category_samples: cli.min_category_samples,
              use_solver: cli.use_solver,
              monotonicity: cli.monotonicity.clone(),
              solver_timeout: cli.solver_timeout,
              solver_gap: cli.solver_gap,
              infer_schema_length: cli.infer_schema_length,
          };

          loop {
              match run_config_menu(config.clone(), columns.clone())? {
                  ConfigResult::Proceed(boxed_cfg) => {
                      let cfg = *boxed_cfg;
                      let target = cfg.target.ok_or_else(|| {
                          anyhow::anyhow!("Target column must be selected before proceeding")
                      })?;

                      return Ok(PipelineConfig {
                          target,
                          missing_threshold: cfg.missing_threshold,
                          gini_threshold: cfg.gini_threshold,
                          gini_bins: cfg.gini_bins,
                          correlation_threshold: cfg.correlation_threshold,
                          columns_to_drop: cfg.columns_to_drop,
                          target_mapping: cfg.target_mapping,
                          weight_column: cfg.weight_column,
                          binning_strategy: cfg.binning_strategy,
                          prebins: cfg.prebins,
                          cart_min_bin_pct: cfg.cart_min_bin_pct,
                          min_category_samples: cfg.min_category_samples,
                          use_solver: cfg.use_solver,
                          monotonicity: cfg.monotonicity,
                          solver_timeout: cfg.solver_timeout,
                          solver_gap: cfg.solver_gap,
                          infer_schema_length: cfg.infer_schema_length,
                      });
                  }
                  ConfigResult::Convert(boxed_cfg) => {
                      let cfg = *boxed_cfg;
                      cli::convert::run_convert(
                          &cfg.input,
                          Some(&cfg.output),
                          cfg.infer_schema_length,
                          true, // fast mode default
                      )?;
                      current_input = cfg.output.clone();
                      columns = get_column_names(&current_input)?;
                      config = cfg;
                  }
                  ConfigResult::Quit => {
                      println!("Cancelled by user.");
                      std::process::exit(0);
                  }
              }
          }
      }

      // Branch 3: Default (Wizard, new behavior)
      match run_wizard(cli)? {
          WizardResult::RunReduction(boxed_cfg) => {
              let cfg = *boxed_cfg;
              let target = cfg.target.ok_or_else(|| {
                  anyhow::anyhow!("Target column must be selected in wizard")
              })?;

              Ok(PipelineConfig {
                  target,
                  missing_threshold: cfg.missing_threshold,
                  gini_threshold: cfg.gini_threshold,
                  gini_bins: cfg.gini_bins,
                  correlation_threshold: cfg.correlation_threshold,
                  columns_to_drop: cfg.columns_to_drop,
                  target_mapping: cfg.target_mapping,
                  weight_column: cfg.weight_column,
                  binning_strategy: cfg.binning_strategy,
                  prebins: cfg.prebins,
                  cart_min_bin_pct: cfg.cart_min_bin_pct,
                  min_category_samples: cfg.min_category_samples,
                  use_solver: cfg.use_solver,
                  monotonicity: cfg.monotonicity,
                  solver_timeout: cfg.solver_timeout,
                  solver_gap: cfg.solver_gap,
                  infer_schema_length: cfg.infer_schema_length,
              })
          }
          WizardResult::RunConversion(conversion_config) => {
              // Run conversion and exit cleanly
              cli::convert::run_convert(
                  &conversion_config.input,
                  Some(&conversion_config.output),
                  conversion_config.infer_schema_length,
                  conversion_config.fast,
              )?;
              println!("Conversion complete: {}", conversion_config.output.display());
              std::process::exit(0);
          }
          WizardResult::Quit => {
              println!("Cancelled by user.");
              std::process::exit(0);
          }
      }
  }
  ```

**Note:** After conversion, the wizard exits cleanly. Chaining conversion-then-reduction is deferred to a future iteration.

**Acceptance Criteria:**
- [ ] `--no-confirm` preserves existing CLI-only behavior
- [ ] `--manual` preserves existing dashboard behavior
- [ ] Default (no flags) runs wizard
- [ ] Wizard result is correctly converted to `PipelineConfig`
- [ ] Conversion result exits cleanly (no pipeline run)

#### Step 4.4: Update Main Pipeline Entry

**File:** `src/main.rs`

**Tasks:**
- Update `main()` function (lines 64-92) to handle wizard-based file selection:
  ```rust
  // Main reduce pipeline - get input from CLI or interactive file selector
  let input = match cli.input() {
      Some(path) => path.clone(),
      None => {
          // If wizard mode (default), wizard handles file selection internally
          // If manual mode or no-confirm, use existing file selector
          if !cli.manual && !cli.no_confirm {
              // In wizard mode, file selection happens inside run_wizard().
              // setup_configuration() receives the resolved path via WizardResult.
              // Skip the file selector here — return early to wizard flow.
              return setup_configuration(cli, &PathBuf::new(), &PathBuf::new());
          } else {
              // Launch interactive file selector for manual/CLI modes
              match run_file_selector()? {
                  FileSelectResult::Selected(path) => path,
                  FileSelectResult::Cancelled => {
                      println!("Cancelled by user.");
                      std::process::exit(0);
                  }
              }
          }
      }
  };
  ```

**Note:** In wizard mode, file selection is handled inside `run_wizard()`. The `WizardResult::RunReduction` variant carries the full `Config` (including input path), so `main()` does not need to resolve the input path separately for wizard mode. The early return above delegates entirely to `setup_configuration()` which calls `run_wizard()` in its default branch.

**Acceptance Criteria:**
- [ ] File selection works for all three modes (CLI, manual, wizard)
- [ ] No duplicate file selector prompts
- [ ] Input path is correctly passed to pipeline

---

### Phase 5: Testing

**Goal:** Comprehensive testing of wizard state machine, validation, and CLI integration.

#### Step 5.1: Unit Tests for Wizard State Machine

**File:** `tests/test_wizard.rs`

**Tasks:**
- Create new integration test file
- Test step sequencing:
  ```rust
  #[test]
  fn test_reduction_path_steps() {
      let mut wizard = WizardState::new();
      wizard.data.task = Some(WizardTask::Reduction);
      wizard.build_steps();

      assert_eq!(wizard.steps.len(), 8);
      assert_eq!(wizard.steps[0], WizardStep::TaskSelection);
      assert_eq!(wizard.steps[1], WizardStep::FileSelection);
      assert_eq!(wizard.steps[2], WizardStep::TargetSelection);
      assert_eq!(wizard.steps[3], WizardStep::MissingThreshold);
      assert_eq!(wizard.steps[4], WizardStep::GiniThreshold);
      assert_eq!(wizard.steps[5], WizardStep::CorrelationThreshold);
      assert_eq!(wizard.steps[6], WizardStep::OptionalSettingsPrompt);
      assert_eq!(wizard.steps[7], WizardStep::Summary);
  }

  #[test]
  fn test_conversion_path_steps() {
      let mut wizard = WizardState::new();
      wizard.data.task = Some(WizardTask::Conversion);
      wizard.build_steps();

      assert_eq!(wizard.steps.len(), 5);
      assert_eq!(wizard.steps[2], WizardStep::OutputPath);
      assert_eq!(wizard.steps[3], WizardStep::ConversionMode);
      assert_eq!(wizard.steps[4], WizardStep::Summary);
  }
  ```
- Test step navigation:
  ```rust
  #[test]
  fn test_step_navigation() {
      let mut wizard = WizardState::new();
      wizard.data.task = Some(WizardTask::Reduction);
      wizard.build_steps();

      assert_eq!(wizard.current_index, 0);

      wizard.next_step().unwrap();
      assert_eq!(wizard.current_index, 1);

      wizard.prev_step().unwrap();
      assert_eq!(wizard.current_index, 0);

      // Cannot go before first step
      assert!(wizard.prev_step().is_err());
  }
  ```
- Test validation:
  ```rust
  #[test]
  fn test_threshold_validation() {
      assert!(validate_threshold(0.5).is_ok());
      assert!(validate_threshold(0.0).is_ok());
      assert!(validate_threshold(1.0).is_ok());
      assert!(validate_threshold(-0.1).is_err());
      assert!(validate_threshold(1.1).is_err());
  }

  #[test]
  fn test_output_path_validation() {
      assert!(validate_parquet_extension("output.parquet").is_ok());
      assert!(validate_parquet_extension("output.csv").is_err());
      assert!(validate_parquet_extension("output").is_err());
  }
  ```

**Acceptance Criteria:**
- [ ] All state machine tests pass
- [ ] Navigation edge cases are covered (first/last step boundaries)
- [ ] Validation tests cover valid and invalid inputs

#### Step 5.2: CLI Integration Tests for --manual Flag

**File:** `tests/test_cli.rs` (create if doesn't exist)

**Tasks:**
- Test CLI argument parsing:
  ```rust
  #[test]
  fn test_manual_flag_parsing() {
      let args = vec!["lophi", "--manual", "--input", "data.csv", "--target", "y"];
      let cli = Cli::try_parse_from(args).unwrap();
      assert!(cli.manual);
  }

  #[test]
  fn test_default_is_wizard_mode() {
      let args = vec!["lophi", "--input", "data.csv", "--target", "y"];
      let cli = Cli::try_parse_from(args).unwrap();
      assert!(!cli.manual);
      assert!(!cli.no_confirm);
  }

  #[test]
  fn test_no_confirm_takes_precedence_over_manual() {
      let args = vec!["lophi", "--manual", "--no-confirm", "--input", "data.csv", "--target", "y"];
      let cli = Cli::try_parse_from(args).unwrap();
      assert!(cli.manual);
      assert!(cli.no_confirm);
      // Per FR-11: --no-confirm takes precedence, no error raised
  }
  ```

**Acceptance Criteria:**
- [ ] `--manual` flag is correctly parsed
- [ ] Default behavior (no flags) is documented as wizard mode
- [ ] Conflicting flags are detected and produce clear error messages

#### Step 5.3: Config Conversion Tests

**File:** `tests/test_wizard.rs`

**Tasks:**
- Test `WizardData` to `Config` conversion:
  ```rust
  #[test]
  fn test_wizard_data_to_result() {
      let wizard_data = WizardData {
          task: Some(WizardTask::Reduction),
          input_path: Some(PathBuf::from("input.csv")),
          columns: vec!["col1".to_string(), "col2".to_string()],
          target: Some("target".to_string()),
          missing_threshold: 0.3,
          gini_threshold: 0.05,
          correlation_threshold: 0.4,
          use_solver: true,
          monotonicity: "none".to_string(),
          weight_column: None,
          drop_columns: vec![],
          infer_schema_length: 10000,
          output_path: Some(PathBuf::from("output.csv")),
          conversion_fast: true,
          target_mapping: None,
      };

      let config = wizard_data.to_result().unwrap();

      assert_eq!(config.input, PathBuf::from("input.csv"));
      assert_eq!(config.target, Some("target".to_string()));
      assert_eq!(config.missing_threshold, 0.3);
      assert_eq!(config.use_solver, true);
  }

  #[test]
  fn test_incomplete_wizard_data_fails() {
      let wizard_data = WizardData {
          task: Some(WizardTask::Reduction),
          input: Some(PathBuf::from("input.csv")),
          target: None,  // Missing required field
          // ... other fields with None
          ..Default::default()
      };

      assert!(wizard_data.to_result().is_err());
  }
  ```

**Acceptance Criteria:**
- [ ] Complete `WizardData` successfully converts to `Config`
- [ ] Incomplete `WizardData` returns clear error message
- [ ] All required fields are validated before conversion

#### Step 5.4: Manual TUI Testing

**Tasks:**
- Test full wizard flow manually:
  1. Run `cargo run` (no args) → wizard should launch
  2. Select "Reduce features" → verify reduction steps appear
  3. Navigate through each step using Enter/Backspace
  4. Test validation errors (invalid thresholds, missing target)
  5. Complete wizard → verify pipeline runs with correct config
  6. Run `cargo run -- --manual` → verify dashboard launches instead
  7. Run with full CLI args and `--no-confirm` → verify pipeline runs without TUI
- Test conversion flow:
  1. Run wizard, select "Convert CSV to Parquet"
  2. Navigate conversion steps
  3. Verify conversion completes and exits
- Test edge cases:
  1. Press Escape at various steps → verify clean exit
  2. Use Backspace to navigate backwards → verify state is preserved
  3. Terminal resize during wizard → verify layout adapts

**Acceptance Criteria:**
- [ ] Wizard launches correctly with no CLI args
- [ ] All steps render properly and navigation works
- [ ] Validation errors are clear and recoverable
- [ ] `--manual` flag correctly launches dashboard
- [ ] `--no-confirm` skips all TUI interactions
- [ ] Wizard exits cleanly on Escape or completion

---

### Phase 6: Documentation & Polish

**Goal:** Update documentation, help text, and README to explain wizard mode.

#### Step 6.1: Update CLAUDE.md

**File:** `/home/neelsbester/lo-phi-main/CLAUDE.md`

**Tasks:**
- Add new section: "## Wizard Mode"
  ```markdown
  ## Wizard Mode

  Lo-phi includes an interactive wizard that guides users through configuration step-by-step. This is the **default** experience when running `lophi` without arguments.

  ### Usage Modes

  Lo-phi offers three usage modes:

  1. **Wizard Mode (default):** Interactive, guided configuration
     ```bash
     lophi  # Launches wizard
     ```

  2. **Manual Dashboard Mode:** Full control over all settings via TUI dashboard
     ```bash
     lophi --manual --input data.csv
     ```

  3. **CLI-Only Mode:** Completely non-interactive, requires all parameters
     ```bash
     lophi --no-confirm --input data.csv --target y --missing-threshold 0.3 ...
     ```

  ### Wizard Flow

  The wizard follows these steps for feature reduction:

  1. **Task Selection:** Choose between reduction or CSV conversion
  2. **File Selection:** Browse and select input file
  3. **Target Selection:** Search and select target column
  4. **Missing Threshold:** Set missing value threshold (one at a time)
  5. **Gini Threshold:** Set Gini/IV threshold
  6. **Correlation Threshold:** Set correlation threshold
  7. **Optional Settings:** Configure solver, weights, columns to drop, schema inference
  8. **Summary:** Review all settings before proceeding

  For CSV conversion, the wizard follows a simpler path:

  1. **Task Selection:** Choose "Convert CSV to Parquet"
  2. **File Selection:** Browse and select input CSV
  3. **Output Path:** Specify output Parquet path
  4. **Mode Selection:** Choose fast (parallel) or memory-efficient (streaming)
  5. **Summary:** Review and start conversion

  ### Architecture

  - **Module:** `src/cli/wizard.rs` (~1000 lines)
  - **State Machine:** Indexed step approach with `Vec<WizardStep>` and `current_index: usize`
  - **Data Accumulation:** `WizardData` struct with `Option<T>` fields
  - **Integration:** `setup_configuration()` in `main.rs` routes to wizard/dashboard/CLI based on flags
  ```

- Update "## Interactive TUI Options" section header to: "## Interactive TUI Options (Dashboard Mode)"

**Acceptance Criteria:**
- [ ] Wizard mode is clearly documented
- [ ] Three usage modes are explained with examples
- [ ] Wizard flow steps are listed
- [ ] Architecture section provides implementation context

#### Step 6.2: Update CLI Help Text

**File:** `src/cli/args.rs`

**Tasks:**
- Update `Cli` struct's long_about:
  ```rust
  #[command(name = "lophi")]
  #[command(author, version, about, long_about = Some("\
  Lo-phi - Feature reduction tool with guided wizard interface\n\n\
  USAGE MODES:\n\
    • Wizard (default):     lophi\n\
    • Manual Dashboard:     lophi --manual --input data.csv\n\
    • CLI Only:             lophi --no-confirm --input data.csv --target y ...\n\n\
  The wizard guides you through configuration step-by-step. Use --manual for full control."))]
  ```

**Acceptance Criteria:**
- [ ] `lophi --help` shows clear explanation of three modes
- [ ] Usage examples are provided
- [ ] Wizard is described as the default

#### Step 6.3: Update README

**File:** `/home/neelsbester/lo-phi-main/README.md`

**Tasks:**
- Add "Quick Start" section before existing usage examples:
  ```markdown
  ## Quick Start

  The easiest way to get started is with the interactive wizard:

  ```bash
  lophi  # No arguments needed!
  ```

  The wizard will guide you through:
  1. Selecting an input file
  2. Choosing a target column
  3. Configuring thresholds
  4. Optional settings (solver, weights, etc.)
  5. Reviewing and confirming your choices

  For experienced users, Lo-phi also offers a manual dashboard (`--manual`) and fully non-interactive CLI mode (`--no-confirm`).
  ```

- Update existing usage examples to show all three modes

**Acceptance Criteria:**
- [ ] README prominently features wizard mode
- [ ] Quick Start section is beginner-friendly
- [ ] All three modes are documented with examples

#### Step 6.4: Add Inline Help Text in Wizard

**File:** `src/cli/wizard.rs`

**Tasks:**
- Add contextual help text to each wizard step:
  - **Task Selection:** "Choose a task to get started"
  - **File Selection:** "Select a CSV or Parquet file to analyze"
  - **Target Selection:** "The target column is used for Gini/IV analysis and preserved in output"
  - **Threshold Configuration:** "Lower missing threshold = more aggressive dropping; Higher correlation threshold = keep more correlated pairs"
  - **Optional Settings:** "These settings are optional—sensible defaults are used if skipped"
  - **Summary:** "Review your choices. Press Enter to start processing."
- Add tooltips or explanation text for complex options (e.g., monotonicity constraint choices)

**Acceptance Criteria:**
- [ ] Each step has clear, concise help text
- [ ] Help text is visible without cluttering the UI
- [ ] Complex options have additional explanation available (e.g., hover or info icon)

---

## Testing Strategy

### Unit Testing

**Scope:** Wizard state machine, validation functions, data conversion

**Location:** `tests/test_wizard.rs`

**Key Test Cases:**
1. **Step Sequencing:**
   - Reduction path builds correct steps
   - Conversion path builds correct steps
   - Step count is accurate after task selection
2. **Navigation:**
   - `next_step()` advances correctly
   - `prev_step()` goes back correctly
   - Boundary conditions (first/last step) handled
3. **Validation:**
   - Threshold validation (0.0-1.0 range)
   - Path validation (file exists, correct extension)
   - Required field validation (target, input)
4. **Data Conversion:**
   - `WizardData` → `Config` conversion with all fields
   - Incomplete `WizardData` returns error
   - CLI args pre-populate `WizardData` correctly

**Acceptance Criteria:** All unit tests pass; coverage >90% for wizard.rs

### CLI Integration Testing

**Scope:** CLI argument parsing, flag behavior, mode selection

**Location:** `tests/test_cli.rs`

**Key Test Cases:**
1. **Flag Parsing:**
   - `--manual` flag correctly sets `cli.manual = true`
   - Default is wizard mode (no flags)
   - `--no-confirm` still works as before
2. **Flag Conflicts:**
   - `--manual` + `--no-confirm`: `--no-confirm` takes precedence per FR-11 (no error raised)
   - CLI args are respected in all modes
3. **Mode Selection:**
   - `setup_configuration()` routes to correct mode
   - Wizard result converts to valid `PipelineConfig`

**Acceptance Criteria:** All CLI tests pass; no regressions in existing CLI behavior

### Manual TUI Testing

**Scope:** Visual rendering, keyboard interaction, user experience

**Checklist:**
- [ ] Wizard launches with `cargo run` (no args)
- [ ] Task selection step renders correctly
- [ ] File selector launches and returns file path
- [ ] Target selection list is searchable and scrollable
- [ ] Threshold fields accept valid input and reject invalid
- [ ] Tab key cycles focus in threshold step
- [ ] Optional settings open correct sub-dialogs
- [ ] Summary displays all collected settings accurately
- [ ] Backspace navigation preserves state (can go back and change settings)
- [ ] Escape quits wizard cleanly at any step
- [ ] Progress indicator (Step X of Y) updates correctly
- [ ] Help text is visible and accurate
- [ ] Terminal resize doesn't break layout
- [ ] `--manual` flag launches dashboard instead
- [ ] `--no-confirm` skips all TUI
- [ ] Conversion flow works end-to-end
- [ ] Post-conversion prompt (if implemented) works correctly

**Acceptance Criteria:** All checklist items pass; no visual glitches or UX issues

### Regression Testing

**Scope:** Existing features still work as before

**Key Test Cases:**
1. **Dashboard Mode:** `--manual` flag produces identical behavior to current default
2. **CLI Mode:** `--no-confirm` with all args produces identical results
3. **Subcommands:** `lophi convert` still works unchanged
4. **Pipeline Output:** Reduced dataset, reports, and zip file are unchanged

**Acceptance Criteria:** All existing integration tests pass; output files match baseline

---

## Risk Assessment

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| **Wizard increases complexity and maintenance burden** | Medium | High | Keep wizard as separate module; well-defined interfaces; comprehensive tests; document state machine |
| **Users accidentally use wizard when they want CLI** | Low | Medium | Clear documentation; `--manual` flag prominently mentioned in help text; wizard can be skipped with `--no-confirm` |
| **Wizard state machine has bugs (infinite loops, crashes)** | High | Medium | Extensive unit tests for state transitions; manual testing; defensive programming (bounds checks) |
| **Terminal compatibility issues (different terminals render differently)** | Medium | Low | Use ratatui's standard components; test on multiple terminals (gnome-terminal, iTerm2, Windows Terminal); fallback to ASCII art if needed |
| **Wizard doesn't handle edge cases (very long column names, huge file lists)** | Medium | Medium | Add truncation/ellipsis for long strings; pagination for long lists; test with extreme inputs |
| **CLI args and wizard state conflict (e.g., CLI specifies target but wizard asks again)** | Low | Medium | Pre-populate wizard from CLI args; skip steps if already provided; clear documentation |
| **Conversion-then-reduction flow is confusing** | Medium | Low | Initially implement as separate tasks (conversion exits); revisit chaining in future iteration |
| **Performance regression in file selection (loading columns)** | Low | Low | Reuse existing `get_column_names()` which is already optimized; no schema changes |
| **Users expect wizard to remember previous settings** | Low | High | Document as future enhancement; wizard uses defaults each time (stateless) |

---

## Rollback Plan

### Scenario 1: Wizard has critical bugs after merge

**Action:**
1. Revert commit introducing wizard integration in `main.rs`
2. Keep `wizard.rs` module but make it inaccessible (remove from `mod.rs` exports)
3. Change default behavior back to dashboard (remove `--manual` flag logic)
4. Hotfix critical bugs in feature branch
5. Re-merge after testing

**Commits to Revert:**
- Phase 4 commit: "feature: integrate wizard into setup_configuration()"
- Partial Phase 4 commit: "feature: add --manual flag and wizard routing"

### Scenario 2: Wizard causes terminal corruption or crashes

**Action:**
1. Add emergency `LOPHI_DISABLE_WIZARD=1` environment variable check in `main.rs`
2. Document workaround: `LOPHI_DISABLE_WIZARD=1 lophi --manual ...`
3. Investigate terminal teardown logic in `wizard.rs`
4. Add panic handler to ensure `teardown_terminal()` always runs
5. Test on affected terminals, adjust rendering code

**No code revert needed** - hotfix in place

### Scenario 3: Users widely complain wizard is confusing

**Action:**
1. Change default to dashboard (`--manual` becomes default)
2. Add `--wizard` flag to opt-in to guided mode
3. Keep wizard code intact for future refinement
4. Gather user feedback and iterate on wizard UX

**Commits to Modify:**
- Phase 4 commit: Change default in `setup_configuration()` from wizard to dashboard

### Scenario 4: Performance regression detected

**Action:**
1. Profile wizard startup time and step transitions
2. Optimize column loading (unlikely culprit, already fast)
3. Lazy-load wizard components (render on demand)
4. Add caching for repeated operations (e.g., column lists)

**Rollback not needed** - performance optimization in place

---

## Dependencies

### Phase Dependencies

- **Phase 1** must complete before **Phase 2** or **Phase 3** (core infrastructure needed)
- **Phase 2** and **Phase 3** are independent (reduction and conversion paths separate)
- **Phase 4** depends on **Phase 1** (CLI integration needs wizard entry point)
- **Phase 5** can run in parallel with **Phase 4** (testing written alongside implementation)
- **Phase 6** should happen last (documentation reflects final implementation)

### External Dependencies

- No new crate dependencies required (ratatui, crossterm, clap already in use)
- Reuses existing functions: `run_file_selector()`, `get_column_names()`, `run_config_menu()` patterns
- Integration points: `setup_configuration()`, `Config` struct, `PipelineConfig` struct

---

## Timeline Estimate

| Phase | Estimated Time | Complexity |
|-------|----------------|------------|
| Phase 1: Core Infrastructure | 4-6 hours | Medium (state machine design) |
| Phase 2: Reduction Path | 6-8 hours | Medium (UI rendering + validation) |
| Phase 3: Conversion Path | 3-4 hours | Low (simpler than reduction) |
| Phase 4: CLI Integration | 3-4 hours | Medium (refactor main.rs carefully) |
| Phase 5: Testing | 4-6 hours | Medium (manual testing time-consuming) |
| Phase 6: Documentation | 2-3 hours | Low (mostly writing) |
| **Total** | **22-31 hours** | **~3-4 days of focused work** |

---

## Success Criteria

### Functional Requirements

- [ ] Wizard launches by default when `lophi` is run with no args
- [ ] Wizard guides user through file selection, target selection, threshold config, optional settings
- [ ] Wizard produces valid `Config` that runs pipeline successfully
- [ ] `--manual` flag preserves existing dashboard behavior
- [ ] `--no-confirm` flag preserves existing CLI-only behavior
- [ ] Conversion path works end-to-end (task selection → file → output → mode → summary)
- [ ] All validation errors are clear and recoverable
- [ ] User can navigate backwards (Backspace) and forwards (Enter) through wizard
- [ ] Escape key quits wizard cleanly at any step

### Non-Functional Requirements

- [ ] Wizard step transitions complete in <100ms (per NFR-2); loading indicator shown for operations exceeding 200ms
- [ ] Terminal rendering is responsive (no flickering or lag)
- [ ] Works on Linux, macOS, Windows (standard terminals)
- [ ] Code follows existing project conventions (formatting, error handling)
- [ ] Test coverage for wizard.rs >90%
- [ ] Documentation is clear and beginner-friendly
- [ ] No breaking changes to existing CLI/dashboard behavior

### User Experience Goals

- [ ] First-time users can complete configuration without reading docs
- [ ] Wizard feels smooth and intuitive (no confusing steps)
- [ ] Progress indicator gives sense of completion (Step X of Y)
- [ ] Help text is visible and helpful
- [ ] Expert users can skip wizard entirely with `--manual` or `--no-confirm`

---

## Post-Implementation Tasks

### Immediate (Before PR Merge)

1. Run full test suite: `cargo test --all-features`
2. Run clippy: `cargo clippy --all-targets --all-features -- -D warnings`
3. Run formatter: `cargo fmt -- --check`
4. Manually test wizard on all three platforms (Linux, macOS, Windows) if possible
5. Update CHANGELOG.md with new feature
6. Record demo GIF/video of wizard flow for README

### Follow-Up (Future Iterations)

1. Add wizard state persistence (remember previous settings) - **Spec 3.1**
2. Add "Skip wizard in future" option (set preference) - **Spec 3.2**
3. Implement conversion-then-reduction chaining - **Spec 3.3**
4. Add wizard progress animation (spinner during file loading) - **Spec 3.4**
5. Improve target mapping wizard step (handle non-binary targets) - **Spec 3.5**
6. Add keyboard shortcuts help panel (press `?` for full keybindings) - **Spec 3.6**

---

## Open Questions

1. **Should conversion exit after completion or prompt for reduction?**
   - **Recommendation:** Exit cleanly for now. Chaining is complex and can be added later.
   - **Decision:** Exit after conversion (Phase 3)

2. **How to handle CLI args that conflict with wizard steps?**
   - **Recommendation:** Pre-populate wizard from CLI args, skip steps that are already provided.
   - **Example:** If `--target y` is provided, skip target selection step.
   - **Decision:** Implement in Phase 1 (wizard initialization)

3. **Should wizard remember settings between runs?**
   - **Recommendation:** No, not in initial implementation. Adds complexity (config file management).
   - **Future:** Add in follow-up spec (state persistence).
   - **Decision:** Wizard is stateless in v0.1.0

4. **What happens if user resizes terminal during wizard?**
   - **Recommendation:** Ratatui handles this automatically; test to confirm.
   - **Decision:** Rely on ratatui's built-in handling

5. **Should `--manual` and `--no-confirm` be mutually exclusive?**
   - **Decision:** No. Per spec FR-11, `--no-confirm` takes precedence when both are provided. No error is raised; the wizard and dashboard are simply bypassed in favor of CLI-only mode.

---

## Appendix: Example Wizard Flow

### Reduction Flow

```
┌────────────────────────────────────────────────────────────────┐
│ Step 1 of 8: What would you like to do?                       │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ > Reduce features in a dataset                                │
│   Analyze and drop features based on missing values,          │
│   Gini/IV, and correlation                                     │
│                                                                │
│   Convert CSV to Parquet                                       │
│   Fast conversion to columnar format                           │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Enter] Select  [↑/↓] Navigate  [Esc] Quit                    │
└────────────────────────────────────────────────────────────────┘

[User presses Enter]

┌────────────────────────────────────────────────────────────────┐
│ Step 2 of 8: Select Input File                                │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ [File selector launches - reusing existing UI]                │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Enter] Select  [↑/↓] Navigate  [Backspace] Back  [Esc] Quit  │
└────────────────────────────────────────────────────────────────┘

[User selects file, columns are loaded in background]

┌────────────────────────────────────────────────────────────────┐
│ Step 3 of 8: Select Target Column                             │
├────────────────────────────────────────────────────────────────┤
│ Search: [          ]                                           │
│                                                                │
│ > age                                                          │
│   income                                                       │
│   credit_score                                                 │
│   loan_status                                                  │
│   ...                                                          │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Enter] Select  [Type] Search  [↑/↓] Navigate                 │
│ [Backspace] Back  [Esc] Quit                                   │
└────────────────────────────────────────────────────────────────┘

[User searches "loan" and selects "loan_status"]

┌────────────────────────────────────────────────────────────────┐
│ Step 4 of 8: Configure Missing Value Threshold                │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ Missing Value Threshold:  [0.30]                               │
│   Features with more than this proportion of missing values   │
│   will be dropped. Default: 0.30 (30%)                         │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Type] Edit  [Enter] Continue  [Esc] Back                      │
└────────────────────────────────────────────────────────────────┘

[User accepts default and presses Enter, similar screens for Gini (Step 5) and Correlation (Step 6)]

┌────────────────────────────────────────────────────────────────┐
│ Step 7 of 8: Optional Settings                                │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ [S] Solver Options        Current: Enabled, Trend: none        │
│ [W] Weight Column         Current: None                        │
│ [D] Drop Columns          Current: None                        │
│ [A] Advanced (Schema)     Current: 10000 rows                  │
│                                                                │
│ [Enter] Continue with these settings                           │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Letter Key] Edit Setting  [Enter] Continue                    │
│ [Backspace] Back  [Esc] Quit                                   │
└────────────────────────────────────────────────────────────────┘

[User presses Enter without changing anything]

┌────────────────────────────────────────────────────────────────┐
│ Step 8 of 8: Summary - Ready to Proceed                       │
├────────────────────────────────────────────────────────────────┤
│ Configuration Summary                                          │
│ ═══════════════════════════════════════════════════════════   │
│ Task:               Feature Reduction                          │
│ Input File:         data/credit_data.csv                       │
│ Target Column:      loan_status                                │
│ Output File:        data/credit_data_reduced.csv               │
│                                                                │
│ Thresholds:                                                    │
│   Missing:          0.30                                       │
│   Gini:             0.05                                       │
│   Correlation:      0.40                                       │
│                                                                │
│ Solver:             Enabled (Trend: none)                      │
│ Weight Column:      None                                       │
│ Columns to Drop:    None                                       │
│ Schema Inference:   10000 rows                                 │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Enter] Start Pipeline  [Backspace] Go Back  [Esc] Cancel     │
└────────────────────────────────────────────────────────────────┘

[User presses Enter, wizard exits, pipeline starts]
```

### Conversion Flow

```
┌────────────────────────────────────────────────────────────────┐
│ Step 1 of 5: What would you like to do?                       │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│   Reduce features in a dataset                                │
│   Analyze and drop features based on missing values,          │
│   Gini/IV, and correlation                                     │
│                                                                │
│ > Convert CSV to Parquet                                       │
│   Fast conversion to columnar format                           │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Enter] Select  [↑/↓] Navigate  [Esc] Quit                    │
└────────────────────────────────────────────────────────────────┘

[User presses Enter]

┌────────────────────────────────────────────────────────────────┐
│ Step 2 of 5: Select Input File                                │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ [File selector - CSV files only]                              │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Enter] Select  [↑/↓] Navigate  [Backspace] Back  [Esc] Quit  │
└────────────────────────────────────────────────────────────────┘

[User selects data.csv]

┌────────────────────────────────────────────────────────────────┐
│ Step 3 of 5: Choose Output Path                               │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ Input:   data/data.csv                                         │
│ Output:  [data/data.parquet]                                   │
│                                                                │
│ Edit path or press Enter to use default                       │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Type] Edit Path  [Enter] Continue                             │
│ [Backspace] Back  [Esc] Quit                                   │
└────────────────────────────────────────────────────────────────┘

[User presses Enter to accept default]

┌────────────────────────────────────────────────────────────────┐
│ Step 4 of 5: Select Conversion Mode                           │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│ > Fast (uses more RAM, parallelizes across all CPUs)          │
│   Recommended for machines with sufficient RAM                │
│   (roughly 2-3x the CSV file size)                             │
│                                                                │
│   Memory-efficient (streaming, single-threaded, low RAM)      │
│   Recommended for large files or limited RAM                  │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Enter] Select  [↑/↓] Navigate  [Backspace] Back  [Esc] Quit  │
└────────────────────────────────────────────────────────────────┘

[User selects Fast mode]

┌────────────────────────────────────────────────────────────────┐
│ Step 5 of 5: Summary - Ready to Convert                       │
├────────────────────────────────────────────────────────────────┤
│ Configuration Summary                                          │
│ ═══════════════════════════════════════════════════════════   │
│ Task:               CSV to Parquet Conversion                  │
│ Input File:         data/data.csv                              │
│ Output File:        data/data.parquet                          │
│ Mode:               Fast (parallel)                            │
│ Schema Inference:   10000 rows                                 │
│                                                                │
├────────────────────────────────────────────────────────────────┤
│ [Enter] Start Conversion  [Backspace] Go Back  [Esc] Cancel   │
└────────────────────────────────────────────────────────────────┘

[User presses Enter, conversion starts, wizard exits]
```

---

## Conclusion

This plan provides a comprehensive roadmap for implementing the TUI Wizard Mode feature. The wizard will:

1. **Lower the barrier to entry** for new users by guiding them through configuration step-by-step
2. **Preserve expert workflows** with `--manual` and `--no-confirm` flags
3. **Maintain code quality** through rigorous testing and adherence to project conventions
4. **Align with constitution principles** (ergonomic TUI/CLI, transparent decision-making)

The implementation is broken into six phases with clear acceptance criteria and estimated timelines. The feature is designed to be **non-breaking** (existing CLI and dashboard behavior unchanged) and **easily reversible** if critical issues arise.

**Next Steps:**
1. Review and approve this plan
2. Begin Phase 1 implementation (core infrastructure)
3. Iterate through phases with testing at each step
4. Conduct final manual testing and documentation updates
5. Submit PR for review

**Estimated Completion:** 3-4 days of focused development + 1 day for review and polish = **~1 week total**
