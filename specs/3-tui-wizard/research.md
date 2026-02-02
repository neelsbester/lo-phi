# TUI Wizard Mode: Technical Research Document

**Version:** 0.1.0
**Date:** 2026-02-02
**Status:** Design Approved

---

## Executive Summary

This document evaluates design decisions for implementing a step-by-step wizard mode as the default Lo-phi experience. The wizard will guide users through configuration one question at a time, while preserving the existing dashboard (expert mode) via a `--manual` flag. The design prioritizes clean separation of concerns, reusability of existing components, and intuitive back navigation.

---

## 1. Wizard State Machine Design

### Decision

Use an **indexed step approach** with a `Vec<WizardStep>` history stack for back navigation.

```rust
struct WizardState {
    current_step: usize,
    step_history: Vec<WizardStep>,
    data: WizardData,  // Accumulated user inputs
}

enum WizardStep {
    TaskSelection,
    FileSelection,
    TargetColumn,
    MissingThreshold,
    GiniThreshold,
    CorrelationThreshold,
    OptionalSettingsPrompt,
    SolverToggle,
    MonotonicitySelection,
    WeightColumn,
    DropColumns,
    SchemaInferenceLength,
    Summary,
}
```

### Rationale

1. **Simple progression logic**: `current_step` is just an index; forward = `current_step += 1`, back = `current_step -= 1`.
2. **Progress indicator**: Total steps known at compile time, easy to render "Step 3 of 8".
3. **Conditional branching**: For optional steps (solver settings, conversion path), insert steps into the history dynamically.
4. **Type safety**: Each step is an enum variant, making pattern matching exhaustive and compiler-checked.

### Alternatives Considered

**Enum-per-step with explicit transitions:**
```rust
enum WizardState {
    TaskSelection { next: Box<WizardState> },
    FileSelection { prev: Box<WizardState>, next: Box<WizardState> },
    // ...
}
```

- **Rejected**: Deeply nested enums make back navigation cumbersome and increase memory overhead. Harder to compute step count for progress indicator.

**Linear state index with `HashMap<usize, StepData>`:**
```rust
struct WizardState {
    current_step: usize,
    step_data: HashMap<usize, StepData>,
}
```

- **Rejected**: Loses type safety—cannot enforce that step 3's data is always a threshold value. HashMap access requires runtime checks.

---

## 2. Integration Strategy with Existing Code

### Decision

Create a **new module `src/cli/wizard.rs`** with a public entry point `run_wizard()` that returns `ConfigResult`.

```rust
// src/cli/wizard.rs
pub fn run_wizard(cli: &Cli) -> Result<ConfigResult> {
    // Initialize wizard state
    // Run event loop (draw -> read key -> mutate state)
    // Return ConfigResult::Proceed(Config) | Convert(Config) | Quit
}
```

Integration in `main.rs`:
```rust
fn setup_configuration(cli: &Cli, input: &Path, output_path: &Path) -> Result<PipelineConfig> {
    if cli.no_confirm {
        // Build config directly from CLI args (current behavior)
    } else if cli.manual {
        // Launch dashboard: run_config_menu()
    } else {
        // Launch wizard: run_wizard()
    }
}
```

### Rationale

1. **Separation of concerns**: Wizard logic is isolated from the 2648-line `config_menu.rs`. Makes both easier to maintain.
2. **Reusability**: Wizard can reuse existing public functions:
   - `run_file_selector()` for file selection step.
   - Column selection drawing code (can extract to a shared helper).
   - `Config` struct is the common output format for both wizard and dashboard.
3. **No breaking changes**: Dashboard code remains untouched. Wizard is purely additive.

### Alternatives Considered

**Extend `config_menu.rs` with wizard variants:**
```rust
enum MenuState {
    // Existing dashboard states
    Main,
    SelectTarget { ... },
    // New wizard states
    WizardTaskSelection,
    WizardMissingThreshold { input: String },
    // ...
}
```

- **Rejected**: Mixing two interaction patterns in one 2600+ line file creates cognitive overload. State transitions become exponentially complex. Testing becomes harder (cannot isolate wizard behavior).

**Separate binary or subcommand:**
```rust
lophi wizard --input data.csv
```

- **Rejected**: Violates requirement FR-1 (wizard is default launch mode). Users shouldn't need to learn a subcommand for the beginner-friendly path.

---

## 3. Back Navigation Pattern

### Decision

Store user inputs in a **`WizardData` struct with `Option<T>` fields**, populated as the wizard progresses. Going back never clears values.

```rust
struct WizardData {
    task: Option<WizardTask>,
    input_file: Option<PathBuf>,
    target_column: Option<String>,
    missing_threshold: Option<f64>,
    gini_threshold: Option<f64>,
    correlation_threshold: Option<f64>,
    use_solver: Option<bool>,
    monotonicity: Option<String>,
    weight_column: Option<String>,
    columns_to_drop: Option<Vec<String>>,
    infer_schema_length: Option<usize>,
    // Conversion-specific fields
    output_path: Option<PathBuf>,
    conversion_mode: Option<ConversionMode>,
}

enum WizardTask {
    FeatureReduction,
    CsvToParquet,
}
```

Back navigation logic:
```rust
match key.code {
    KeyCode::Esc | KeyCode::Left => {
        if state.current_step > 0 {
            state.current_step -= 1;
            // Values remain in state.data
        }
    }
    KeyCode::Enter => {
        // Validate and store input for current step
        store_value_for_step(&mut state.data, current_step, user_input)?;
        state.current_step += 1;
    }
}
```

### Rationale

1. **Type safety**: Each field has a known type. No runtime casting.
2. **Explicit initialization**: All fields start as `None`, preventing uninitialized reads.
3. **Persistence**: Going back to a step pre-fills the input field with the stored value.
4. **Validation boundary**: Can enforce required fields at summary step: `data.target_column.ok_or("Target not selected")?`.

### Alternatives Considered

**`HashMap<StepId, Box<dyn Any>>`:**
```rust
step_data.insert(StepId::MissingThreshold, Box::new(0.3_f64));
let threshold: f64 = *step_data.get(&StepId::MissingThreshold)?
    .downcast_ref::<f64>()?;
```

- **Rejected**: Loses compile-time type safety. Runtime panics if downcast fails. More code to maintain.

**Struct per step with nested `Option<NextStep>`:**
```rust
struct MissingThresholdStep {
    value: f64,
    next: Option<Box<GiniThresholdStep>>,
}
```

- **Rejected**: Requires deep cloning for back navigation. Cannot easily jump forward/backward by multiple steps.

---

## 4. Progress Indicator Rendering

### Decision

Render a **top bar** showing step number, total steps, and step title.

```
╭────────────────────────────────────────────────────────────────╮
│ Step 3 of 8 — Gini Threshold                                   │
╰────────────────────────────────────────────────────────────────╯

  The Gini threshold controls which features are kept based on
  their predictive power (via Information Value analysis).

  Features with Gini below this threshold will be dropped.

  Default: 0.05 (5% Gini coefficient)

  ┌──────────────────────────────────────────────────────────────┐
  │ Enter Gini threshold (0.0 - 1.0): 0.05█                      │
  └──────────────────────────────────────────────────────────────┘

  [Enter] Continue   [Esc] Back   [Q] Quit
```

Implementation:
```rust
fn render_progress_bar(frame: &mut Frame, area: Rect, step: usize, total: usize, title: &str) {
    let text = format!("Step {} of {} — {}", step + 1, total, title);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(paragraph, area);
}
```

### Rationale

1. **Visibility**: Top bar is always visible, unobstructed by content.
2. **Consistency**: Matches dashboard's top-bar design pattern (existing code already uses top bars for titles).
3. **Clarity**: `Step 3 of 8` immediately communicates progress. Step title reinforces current context.

### Alternatives Considered

**Bottom bar:**
```
  [←] Back | Step 3/8 | [→] Next | [Q] Quit
```

- **Rejected**: Bottom bar already used for keyboard shortcuts in dashboard. Wizard needs space for shortcuts + progress, which gets cramped.

**Integrated into step content:**
```
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Gini Threshold (Step 3 of 8)
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

- **Rejected**: Harder to maintain consistent positioning across different step layouts. Top bar provides a fixed visual anchor.

**Text-only (no box):**
```
Step 3 of 8 — Gini Threshold

The Gini threshold controls...
```

- **Considered**: Simpler rendering but less visually distinct. Box provides clear separation from step content.

---

## 5. Target Column + Data Loading Timing

### Decision

**Load column names during file selection** (lightweight schema inference). Full data loading remains post-wizard. Non-binary target mapping also remains post-wizard (consistent with current flow).

```rust
// In wizard's file selection step
let columns = get_column_names(&selected_file)?;  // Only reads schema, not data
state.data.available_columns = Some(columns);

// Later, in main.rs after wizard completes
let df = load_dataset_with_progress(&input, config.infer_schema_length)?;
// Then validate target column and run mapping selector if needed
```

### Rationale

1. **Consistency with existing architecture**: Current code already loads column names before config menu (`get_column_names()` in `main.rs:277`). Wizard follows the same pattern.
2. **Performance**: `get_column_names()` only reads file headers/schema (milliseconds), not the full dataset (seconds/minutes).
3. **UX benefit**: Users can select target column immediately after choosing file, without waiting for full data load.
4. **No premature loading**: Full data load only happens once after all configuration is finalized, avoiding wasted work if user quits wizard.

### Alternatives Considered

**Load full data during wizard target step:**
```rust
// In wizard target selection step
let df = load_dataset_with_progress(&selected_file, config.infer_schema_length)?;
let columns = df.get_column_names();
```

- **Rejected**: Wizard should feel instant (NFR-2). Loading a 2GB CSV mid-wizard violates user expectations. What if user goes back and changes file? Must reload.

**Defer target selection to post-wizard:**
```rust
// Wizard returns ConfigResult::Proceed(partial_config)
// main.rs then calls run_target_mapping_selector()
```

- **Rejected**: Wizard loses key benefit of guiding users through all config. User expects wizard to handle target selection (FR-5 requirement).

**Pre-load data before wizard starts:**
```rust
// Before launching wizard
let df = load_dataset_with_progress(&input, ...)?;
run_wizard(cli, df.get_column_names())?;
```

- **Rejected**: Long startup delay before wizard even appears. Poor UX. Also wastes work if user selects "CSV to Parquet" task (no need to load data).

---

## 6. CLI Flag Integration

### Decision

Add `manual: bool` to `Cli` struct. Update `setup_configuration()` with three-way branching:

```rust
// src/cli/args.rs
pub struct Cli {
    // ... existing fields

    /// Skip interactive confirmation prompts
    #[arg(long, default_value = "false")]
    pub no_confirm: bool,

    /// Launch expert mode dashboard instead of wizard
    #[arg(long, default_value = "false")]
    pub manual: bool,
}

// src/main.rs
fn setup_configuration(cli: &Cli, input: &Path, output_path: &Path) -> Result<PipelineConfig> {
    if cli.no_confirm {
        // Existing: Build config from CLI args only (no TUI)
        // PRIORITY 1: Highest precedence
    } else if cli.manual {
        // PRIORITY 2: Launch dashboard (expert mode)
        let columns = get_column_names(input)?;
        let config = Config { /* ... */ };
        match run_config_menu(config, columns)? {
            ConfigResult::Proceed(cfg) => convert_to_pipeline_config(cfg),
            ConfigResult::Quit => std::process::exit(0),
            // ... handle conversion case
        }
    } else {
        // PRIORITY 3: Default path — launch wizard
        run_wizard(cli, input, output_path)
    }
}
```

### Rationale

1. **Backward compatibility**: `--no-confirm` continues to work exactly as before (highest priority).
2. **Clear naming**: `--manual` communicates "manual configuration" (expert dashboard) vs. "guided wizard".
3. **Precedence hierarchy**: `no_confirm` > `manual` > default wizard. Prevents conflicting flags.
4. **No mode selection screen**: Satisfies FR-1 requirement. Wizard launches directly by default.

### Alternatives Considered

**Rename `--no-confirm` to `--expert`:**
```rust
#[arg(long)]
pub expert: bool,  // Launch dashboard directly
```

- **Rejected**: Breaking change. Existing scripts/CI pipelines use `--no-confirm`. Also, `--no-confirm` specifically means "skip all TUI" (not just wizard), so renaming loses semantic clarity.

**New flag `--wizard` to opt into wizard:**
```rust
lophi --wizard --input data.csv
```

- **Rejected**: Violates FR-1. Default experience should be wizard, not opt-in.

**Subcommand approach:**
```bash
lophi wizard    # Launch wizard
lophi config    # Launch dashboard
lophi reduce    # CLI-only (--no-confirm behavior)
```

- **Rejected**: Adds complexity for users. Breaks existing workflows. Requires restructuring entire CLI.

**Interactive mode selection at launch:**
```
╭─────────────────────────────────────╮
│ How would you like to configure?   │
│  [1] Guided Wizard (recommended)   │
│  [2] Expert Dashboard               │
╰─────────────────────────────────────╯
```

- **Rejected**: Adds extra interaction before wizard starts (violates NFR-2: "instant"). Power users already have `--manual` flag.

---

## 7. Conversion Wizard Path

### Decision

**Branch within the same wizard** after task selection. Conversion path: File → Output Path → Mode → Execute.

```rust
// After TaskSelection step
match state.data.task {
    Some(WizardTask::FeatureReduction) => {
        state.step_history.extend([
            WizardStep::FileSelection,
            WizardStep::TargetColumn,
            WizardStep::MissingThreshold,
            // ... full reduction flow
        ]);
    }
    Some(WizardTask::CsvToParquet) => {
        state.step_history.extend([
            WizardStep::FileSelection,
            WizardStep::ConversionOutputPath,
            WizardStep::ConversionMode,
            WizardStep::ConversionSummary,
        ]);
    }
    None => unreachable!(),
}
```

Each path has its own summary step that produces the appropriate `ConfigResult`:
- Reduction path: `ConfigResult::Proceed(Config)`
- Conversion path: `ConfigResult::Convert(Config)`

### Rationale

1. **Reuse file selector**: Both paths need file selection. Single implementation.
2. **Clear branching point**: User sees task choice, then immediately flows into task-specific steps.
3. **Shorter conversion flow**: Only 4 steps vs. 8+ for reduction. Conversion users get a fast path.
4. **Single entry point**: `run_wizard()` handles both cases. No need for `run_conversion_wizard()` vs. `run_reduction_wizard()`.

### Alternatives Considered

**Separate `run_conversion_wizard()` function:**
```rust
match task_selection() {
    Task::Reduction => run_reduction_wizard(),
    Task::Conversion => run_conversion_wizard(),
}
```

- **Rejected**: Duplicates file selection logic, progress indicator rendering, and quit confirmation. Violates DRY principle.

**Skip wizard for conversion:**
```bash
lophi convert input.csv --output out.parquet --fast
```

- **Rejected**: Current behavior already exists via `lophi convert` subcommand. Wizard should guide users who don't know about subcommands (consistency with FR-9).

**Unified flow (no task selection):**
```
Wizard always asks: Do you also want to convert CSV to Parquet? [Y/n]
```

- **Rejected**: Confusing for users who only want reduction. Task selection makes intent clear upfront.

---

## 8. Testing Strategy for Wizard

### Decision

**Extract wizard state logic into pure functions; test those. Integration tests for CLI flag parsing.**

```rust
// Testable pure functions
fn validate_threshold_input(input: &str) -> Result<f64, ValidationError> {
    let value: f64 = input.parse()
        .map_err(|_| ValidationError::NotNumeric)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(ValidationError::OutOfRange);
    }
    Ok(value)
}

fn next_step(current: WizardStep, data: &WizardData) -> WizardStep {
    match current {
        WizardStep::OptionalSettingsPrompt => {
            if data.configure_optional == Some(true) {
                WizardStep::SolverToggle
            } else {
                WizardStep::Summary
            }
        }
        // ...
    }
}

// Unit tests (no TUI)
#[test]
fn test_threshold_validation() {
    assert_eq!(validate_threshold_input("0.5"), Ok(0.5));
    assert!(validate_threshold_input("1.5").is_err());
    assert!(validate_threshold_input("abc").is_err());
}

#[test]
fn test_step_transitions() {
    let mut data = WizardData::default();
    data.configure_optional = Some(false);
    assert_eq!(
        next_step(WizardStep::OptionalSettingsPrompt, &data),
        WizardStep::Summary
    );
}

// Integration tests (CLI flag parsing)
#[test]
fn test_manual_flag_launches_dashboard() {
    // Use clap's parse_from() to test CLI arg handling
    let cli = Cli::parse_from(["lophi", "--manual", "--input", "test.csv"]);
    assert!(cli.manual);
    assert!(!cli.no_confirm);
}
```

**TUI rendering code remains untested** (requires mocking terminal, brittle to styling changes).

### Rationale

1. **Pragmatic approach**: TUI code is inherently hard to test (requires terminal mocking). Testing business logic separately gives high confidence.
2. **Fast test suite**: Pure function tests run in milliseconds. No terminal setup/teardown.
3. **Maintainability**: Decoupling validation/transitions from rendering makes both easier to refactor.
4. **Coverage where it matters**: Critical paths (validation, state transitions, flag parsing) are covered. Cosmetic rendering bugs are caught manually.

### Alternatives Considered

**Full TUI integration tests with `ratatui-test`:**
```rust
#[test]
fn test_wizard_flow() {
    let mut terminal = TestTerminal::new()?;
    let mut wizard = WizardState::new();

    // Simulate keypresses
    wizard.handle_key(KeyCode::Char('1'));  // Select task
    wizard.handle_key(KeyCode::Enter);
    // ... simulate entire flow

    let output = terminal.snapshot();
    assert!(output.contains("Step 3 of 8"));
}
```

- **Rejected**: Extremely brittle (breaks when styling changes). Slow (must render to virtual terminal). Marginal value over unit testing transitions.

**No tests (manual testing only):**
- **Rejected**: Violates constitution principle #5 (rigorous testing). State machine bugs are easy to introduce and hard to debug without tests.

**Property-based testing for state transitions:**
```rust
#[test]
fn prop_back_navigation_preserves_data() {
    proptest!(|(steps: Vec<WizardStep>, values: Vec<StepValue>)| {
        // Assert going forward then back N times preserves values
    });
}
```

- **Considered but deferred**: High value for back navigation testing, but requires significant setup (custom `Arbitrary` implementations for all step types). Recommend as future enhancement.

---

## Implementation Roadmap

### Phase 1: Core Wizard Module (2-3 days)
- [ ] Create `src/cli/wizard.rs` with basic state machine
- [ ] Implement indexed step navigation with history stack
- [ ] Add `WizardData` struct with all config fields
- [ ] Implement progress indicator rendering
- [ ] Add quit confirmation dialog

### Phase 2: Wizard Steps (3-4 days)
- [ ] Task selection step (Reduction vs. Conversion)
- [ ] File selection (reuse `run_file_selector()`)
- [ ] Target column selection (adapt existing column selector)
- [ ] Threshold input steps with validation (Missing, Gini, Correlation)
- [ ] Optional settings prompt + sub-steps (Solver, Weight, Drop, Schema)
- [ ] Summary/confirmation step

### Phase 3: Conversion Path (1 day)
- [ ] Conversion-specific steps (Output, Mode)
- [ ] Branch logic after task selection
- [ ] Conversion summary step

### Phase 4: CLI Integration (1 day)
- [ ] Add `manual` flag to `Cli` struct
- [ ] Update `setup_configuration()` branching
- [ ] Update help text and README

### Phase 5: Testing (2 days)
- [ ] Unit tests for validation functions
- [ ] Unit tests for state transitions
- [ ] Integration tests for CLI flags
- [ ] Manual TUI testing (full flow, back navigation, error cases)

### Phase 6: Documentation (1 day)
- [ ] Update CLAUDE.md with wizard architecture
- [ ] Add wizard keyboard shortcuts to help text
- [ ] Update README with wizard examples

**Total Estimate:** 10-12 days

---

## Open Questions & Future Enhancements

### Open Questions
1. **Should the wizard support resuming from a saved state?** (e.g., save wizard progress to `~/.lophi/wizard-state.json` and resume on crash)
   - **Recommendation**: Defer to future iteration. Adds complexity; wizard flow is short enough to restart.

2. **Should optional settings have a "Configure later" option that jumps to summary?**
   - **Recommendation**: Yes. Add "Skip optional settings" choice at `OptionalSettingsPrompt` step.

3. **Should the summary step allow editing individual values without going back?**
   - **Recommendation**: No. Back navigation is sufficient. Adding inline editing adds complexity.

### Future Enhancements
- Property-based testing for state transitions (validate back navigation at scale)
- Wizard persistence (save/resume state)
- Inline help tooltips (press `?` for detailed explanations)
- Colorblind-friendly theme option
- Wizard flow for future analysis types (e.g., PSI analysis, once implemented)

---

## Appendix: Key Code References

- **Existing state machine pattern**: `src/cli/config_menu.rs:69-121` (`MenuState` enum)
- **Target mapping selector**: `src/cli/config_menu.rs:161-172` (`run_target_mapping_selector()`)
- **Column selection UI**: `src/cli/config_menu.rs:1800-1900` (can be extracted to shared helper)
- **File selector**: `src/cli/config_menu.rs:2200-2400` (`run_file_selector()`)
- **Config struct**: `src/cli/config_menu.rs:27-66`
- **CLI args**: `src/cli/args.rs:10-120`
- **Setup configuration logic**: `src/main.rs:231-330`
