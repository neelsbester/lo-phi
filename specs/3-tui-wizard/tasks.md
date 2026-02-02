# Tasks: TUI Wizard Mode

**Plan Version:** 0.1.0
**Date:** 2026-02-02

---

## Task Categories

Tasks are categorized by the constitution principle they primarily serve:

- **UX:** Ergonomic TUI/CLI (Principle 4) — Primary focus for this feature
- **TRANS:** Transparent Decision-Making (Principle 3)
- **TEST:** Rigorous Testing (Principle 5)

**Task Markers:**
- **[P]** — Parallelizable: this task can be executed in parallel with other tasks in the same phase
- **[US*N*]** — User Story number this task serves

---

## Phase 1: Setup

**Goal:** Create the wizard module skeleton and register it in the CLI module system.

- [x] T001 Create wizard module file at `src/cli/wizard.rs` with module-level documentation explaining the wizard's purpose and architecture
- [x] T002 Register wizard module in `src/cli/mod.rs` by adding `pub mod wizard;` and exporting `pub use wizard::{run_wizard, WizardResult};`

---

## Phase 2: Foundational — Core Wizard Infrastructure

**Goal:** Implement the state machine, terminal management, rendering framework, and main event loop. All subsequent user story phases depend on this.

- [x] T003 Define core types (`WizardResult`, `TaskSelection`, `WizardData`, `WizardStep`, `WizardState`, `StepAction`, `ThresholdField`) in `src/cli/wizard.rs` per the data model spec
- [x] T004 Implement `WizardData::default()` with sensible defaults (missing: 0.30, gini: 0.05, correlation: 0.40, use_solver: true, monotonicity: "none", infer_schema_length: 10000) in `src/cli/wizard.rs`
- [x] T005 Implement `WizardState::new()`, `WizardState::build_steps()`, `WizardState::next_step()`, `WizardState::prev_step()`, `WizardState::current_step()`, and `WizardState::is_last_step()` step sequencing logic in `src/cli/wizard.rs`
- [x] T006 Implement `setup_terminal()` and `teardown_terminal()` functions with panic-safe cleanup in `src/cli/wizard.rs`
- [x] T007 Implement public `run_wizard(cli: &Cli) -> Result<WizardResult>` entry point that pre-populates `WizardData` from CLI args in `src/cli/wizard.rs`
- [x] T008 Implement `run_wizard_loop()` main event loop (draw → read key → handle event → transition) in `src/cli/wizard.rs`
- [x] T009 Implement `render_wizard()` base rendering framework with three-zone layout (progress bar top, main content center, help bar bottom) in `src/cli/wizard.rs`. The progress bar must dynamically compute total step count from `wizard.steps.len()` to handle branching (reduction: 8+ steps, conversion: 5 steps, optional settings: varies).
- [x] T010 Implement `get_step_title()` and `render_step()` dispatch function that routes to step-specific renderers in `src/cli/wizard.rs`
- [x] T067 [P] Implement dynamic step count recalculation in progress bar: when optional steps are inserted/removed (OptionalSettingsPrompt "Yes"/"No") or TargetMapping is conditionally added, the progress indicator ("Step X of Y") must update Y accordingly in `src/cli/wizard.rs`
- [x] T068 Implement loading indicator widget for file/column loading operations: if `get_column_names()` or file loading exceeds 200ms, display a spinner or progress indicator (per NFR-2) in `src/cli/wizard.rs`
- [x] T011 Implement threshold validation function `validate_threshold(value: f64) -> Result<()>` ensuring 0.0-1.0 range, and schema inference validation ensuring value == 0 or >= 100, in `src/cli/wizard.rs`

---

## Phase 3: User Story 1 — Reduction Pipeline Wizard (P1)

**Story Goal:** A first-time user can configure and launch the full feature reduction pipeline through the wizard without external documentation.

**Independent Test Criteria:** User can launch `cargo run`, select "Feature Reduction", pick a file, select a target column, accept/edit thresholds, optionally configure solver/weights/drop columns, review a summary, and confirm to start the pipeline.

### Task Selection Step

- [x] T012 [US1] Implement `render_task_selection()` showing "Reduce features" and "Convert CSV to Parquet" options with descriptions in `src/cli/wizard.rs`
- [x] T013 [US1] Implement `handle_task_selection_event()` with Up/Down navigation, Enter to select (sets task + rebuilds steps), Q to quit in `src/cli/wizard.rs`

### File Selection Step

- [x] T014 [US1] Implement file selection step integration that reuses `run_file_selector()` from `config_menu.rs`, loads column names via `get_column_names()`, and derives default output path in `src/cli/wizard.rs`
- [x] T015 [US1] Implement CLI pre-population skip logic: if `wizard.data.input` is already set from CLI args, skip interactive file selection and confirm the file in `src/cli/wizard.rs`

### Target Selection Step

- [x] T016 [US1] Implement `render_target_selection()` with searchable/filterable column list, current selection display, and help text explaining target column purpose in `src/cli/wizard.rs`
- [x] T017 [US1] Implement `handle_target_selection_event()` with search input, Up/Down navigation, Enter to select, Esc to go back, and validation that target exists in columns in `src/cli/wizard.rs`

### Target Mapping Step (Always Shown)

- [x] T057 [US1] Implement `render_target_mapping()` showing unique target values with two selection areas (event value, non-event value) and help text explaining binary mapping. For binary (0/1) targets, pre-select 1=event and 0=non-event so the user can accept with Enter or override in `src/cli/wizard.rs`
- [x] T058 [US1] Implement `handle_target_mapping_event()` with navigation between event/non-event selection, validation that both are selected and differ, Enter to advance, Esc to show quit confirmation in `src/cli/wizard.rs`
- [x] T059 [US1] Include TargetMapping as a permanent step in `build_steps()` for the reduction path (always shown after TargetSelection). Load unique target values from the dataset during the TargetSelection-to-TargetMapping transition in `src/cli/wizard.rs`

### Threshold Steps (One Per Step — FR-6)

- [x] T018 [US1] Implement `render_missing_threshold()`, `render_gini_threshold()`, and `render_correlation_threshold()` — each showing a single editable field with default value and explanatory help text (one threshold per wizard step, per FR-6) in `src/cli/wizard.rs`
- [x] T019 [US1] Implement `handle_missing_threshold_event()`, `handle_gini_threshold_event()`, and `handle_correlation_threshold_event()` — each with typing to edit, Enter to validate (0.0-1.0) and advance, Esc to go back, inline error display for invalid inputs in `src/cli/wizard.rs`

### Optional Configuration Step

- [x] T020 [US1] Implement `render_optional_config()` showing menu of optional settings ([S] Solver, [W] Weight, [D] Drop columns, [A] Advanced) with current values displayed next to each option in `src/cli/wizard.rs`
- [x] T021 [US1] Implement `handle_optional_config_event()` with letter-key sub-dialogs for each optional setting, Enter to continue with current values, Esc to go back in `src/cli/wizard.rs`
- [x] T022 [US1] Implement solver options sub-dialog (toggle on/off + monotonicity selection from ["none", "ascending", "descending", "peak", "valley", "auto"]) in `src/cli/wizard.rs`
- [x] T023 [US1] Implement weight column selector sub-dialog (searchable column list with "None" option) in `src/cli/wizard.rs`
- [x] T024 [US1] Implement drop columns multi-select sub-dialog (searchable column list with checkboxes, Space to toggle, Enter to confirm) in `src/cli/wizard.rs`
- [x] T025 [US1] Implement schema inference sub-dialog (numeric input with validation: 0 for full scan or >= 100) in `src/cli/wizard.rs`

### Quit Confirmation Dialog

- [x] T060 [US1] Implement quit confirmation overlay dialog: when user presses `Q` at any step, display "Are you sure you want to quit? [Y] Yes [N] No" overlay; Y returns `StepAction::Quit`, N dismisses dialog and returns to current step in `src/cli/wizard.rs`

### Reduction Summary Step

- [x] T026 [US1] Implement `render_reduction_summary()` displaying all collected settings in a formatted table (task, file, target, thresholds, solver, weight, drop columns, schema inference) in `src/cli/wizard.rs`
- [x] T027 [US1] Implement `handle_reduction_summary_event()` with Enter to convert `WizardData` → `Config` and return `WizardResult::RunReduction`, Esc to go back, Q to quit in `src/cli/wizard.rs`
- [x] T028 [US1] Implement `WizardData::to_result()` conversion method that returns `WizardResult::RunReduction(Box<Config>)` or `WizardResult::RunConversion(Box<ConversionConfig>)`, using CLI defaults for non-wizard parameters (gini_bins, binning_strategy, prebins, etc.) in `src/cli/wizard.rs`

---

## Phase 4: User Story 2 — CSV-to-Parquet Conversion Wizard (P2)

**Story Goal:** A user can select CSV-to-Parquet conversion and complete the conversion flow through the wizard.

**Independent Test Criteria:** User can launch wizard, select "Convert CSV to Parquet", pick a CSV file, confirm/edit output path, choose conversion mode (fast/streaming), review summary, and start conversion.

### Conversion Output Path Step

- [x] T029 [US2] Implement `render_conversion_output()` showing input file path and editable output path field with default (input path with `.parquet` extension) in `src/cli/wizard.rs`
- [x] T030 [US2] Implement `handle_conversion_output_event()` with typing to edit path, Enter to validate (must end in `.parquet`, parent dir must exist) and advance, Esc to go back in `src/cli/wizard.rs`

### Conversion Mode Step

- [x] T031 [US2] Implement `render_conversion_mode()` showing two options with descriptions: "Fast (parallel, uses more RAM)" and "Memory-efficient (streaming, low RAM)" in `src/cli/wizard.rs`
- [x] T032 [US2] Implement `handle_conversion_mode_event()` with Up/Down to change selection, Enter to set `convert_fast` and advance in `src/cli/wizard.rs`

### Conversion Summary Step

- [x] T033 [US2] Implement `render_conversion_summary()` displaying task, input file, output file, mode, and schema inference settings in `src/cli/wizard.rs`
- [x] T034 [US2] Implement `handle_conversion_summary_event()` with Enter to return `WizardResult::RunConversion`, Esc to go back, Q to quit in `src/cli/wizard.rs`

---

## Phase 5: User Story 3 — CLI Integration & Mode Switching (P1)

**Story Goal:** The wizard integrates seamlessly with the existing CLI, with `--manual` for expert dashboard and default for wizard. No breaking changes to existing behavior.

**Independent Test Criteria:** `cargo run` launches wizard; `cargo run -- --manual --input data.csv` launches dashboard; `cargo run -- --no-confirm --input data.csv --target y` runs pipeline directly. All three modes produce correct `PipelineConfig`.

### Add --manual Flag

- [x] T035 [US3] Add `manual: bool` field with `#[arg(long, default_value = "false")]` to `Cli` struct in `src/cli/args.rs` with help text explaining it launches expert dashboard mode
- [x] T036 [US3] Add validation in `src/main.rs` that `--manual` and `--no-confirm` together is handled (spec says `--no-confirm` takes precedence)

### Update setup_configuration() Orchestration

- [x] T037 [US3] Refactor `setup_configuration()` in `src/main.rs` to three-way branching: `no_confirm` → CLI-only, `manual` → dashboard (`run_config_menu`), default → wizard (`run_wizard`)
- [x] T038 [US3] Implement wizard result handling in `setup_configuration()`: convert `WizardResult::RunReduction` to `PipelineConfig`, handle `WizardResult::RunConversion` by calling `run_convert()` with `ConversionConfig` fields and exiting cleanly, handle `WizardResult::Quit` by exiting in `src/main.rs`

### Update Main Pipeline Entry

- [x] T039 [US3] Update `main()` function in `src/main.rs` to handle wizard-based file selection (wizard handles file selection internally for default mode, file selector for manual mode) without duplicate prompts

### Update CLI Help Text

- [x] T040 [US3] Update `Cli` struct `long_about` in `src/cli/args.rs` to document all three usage modes (wizard default, manual dashboard, CLI-only) with examples

---

## Phase 6: User Story 4 — Testing (P1)

**Story Goal:** Comprehensive automated tests cover wizard state machine, validation, CLI flag parsing, and config conversion.

**Independent Test Criteria:** `cargo test --all-features` passes with all new wizard tests; no regressions in existing tests; coverage >90% for wizard state logic and validation.

### Unit Tests for Wizard State Machine

- [x] T041 [US4] Create `tests/test_wizard.rs` with test for reduction path step sequencing (9 steps in correct order: TaskSelection, FileSelection, TargetSelection, TargetMapping, MissingThreshold, GiniThreshold, CorrelationThreshold, OptionalSettingsPrompt, Summary)
- [x] T042 [US4] Add test for conversion path step sequencing (5 steps in correct order) in `tests/test_wizard.rs`
- [x] T043 [US4] Add tests for step navigation: `next_step()` advances, `prev_step()` goes back, boundary conditions (before first, after last) return errors in `tests/test_wizard.rs`
- [x] T044 [US4] Add tests for threshold validation: valid values (0.0, 0.5, 1.0), invalid values (-0.1, 1.1, NaN), non-numeric input in `tests/test_wizard.rs`
- [x] T045 [US4] Add tests for schema inference validation: valid (0, 100, 10000), invalid (50, negative) in `tests/test_wizard.rs`
- [x] T046 [P] [US4] Add tests for output path validation: `.parquet` extension required, parent dir exists in `tests/test_wizard.rs`

### Config Conversion Tests

- [x] T047 [US4] Add test for complete `WizardData::to_result()` conversion returning `WizardResult::RunReduction` with all fields populated in `tests/test_wizard.rs`
- [x] T048 [US4] Add test for incomplete `WizardData::to_result()` returning error when required fields are missing (no target, no input) in `tests/test_wizard.rs`
- [x] T049 [P] [US4] Add test for CLI args pre-population into `WizardData` (input, target, thresholds from CLI args are set correctly) in `tests/test_wizard.rs`

### CLI Integration Tests

- [x] T050 [US4] Add test for `--manual` flag parsing in `tests/test_cli.rs` (verify `cli.manual == true`)
- [x] T051 [US4] Add test for default mode (no flags) verifying `cli.manual == false && cli.no_confirm == false` in `tests/test_cli.rs`
- [x] T052 [P] [US4] Add test for `--no-confirm` precedence over `--manual` behavior in `tests/test_cli.rs`

### Edge Case Tests (Constitution P5)

- [x] T061 [P] [US4] Add test for wizard with empty column list (file loads but has no columns) — should display error and stay on file selection step in `tests/test_wizard.rs`
- [x] T062 [P] [US4] Add test for wizard with extremely long column names (>200 chars) — should truncate display without breaking layout in `tests/test_wizard.rs`
- [x] T063 [P] [US4] Add test for wizard threshold validation with edge inputs: empty string, whitespace-only, NaN, Infinity, negative zero in `tests/test_wizard.rs`
- [x] T064 [P] [US4] Add test for wizard step navigation at boundaries after optional steps are dynamically inserted/removed in `tests/test_wizard.rs`

### Non-Functional Requirement Tests

- [x] T065 [US4] Add test or manual verification step that wizard step transitions complete rendering in under 100ms (NFR-2 compliance) in `tests/test_wizard.rs` or manual test checklist
- [x] T066 [US4] Add test verifying wizard renders correctly at minimum terminal size of 80x24 (NFR-4 compliance) — verify no layout overflow or panic at boundary size in `tests/test_wizard.rs`

---

## Phase 7: User Story 5 — Documentation & Polish (P2)

**Story Goal:** Documentation accurately describes wizard mode, all three usage modes are explained, and inline help text guides users through each step.

**Independent Test Criteria:** `lophi --help` shows three modes; CLAUDE.md documents wizard architecture; README includes quick start with wizard.

### Update CLAUDE.md

- [x] T053 [US5] Add "Wizard Mode" section to `CLAUDE.md` documenting: three usage modes with examples, wizard flow steps for reduction and conversion, wizard architecture (module, state machine, data accumulation, integration point)
- [x] T054 [US5] Update "Interactive TUI Options" section header in `CLAUDE.md` to "Interactive TUI Options (Dashboard Mode)" to distinguish from wizard mode

### Update README

- [x] T055 [US5] Add "Quick Start" section to `README.md` showing wizard as the easiest way to get started, with the three usage modes documented

### Add Inline Help Text

- [x] T056 [US5] Add contextual help text to each wizard step in `src/cli/wizard.rs`: task selection, file selection, target selection, threshold config, optional settings, and summary (per plan step 6.4)

---

## Dependency Graph

```
Phase 1 (Setup)
  T001 → T002

Phase 2 (Foundational)
  T002 → T003 → T004
  T003 → T005
  T003 → T006
  T005 → T007
  T006 → T007
  T007 → T008
  T003 → T009
  T009 → T010
  T003 → T011

Phase 3 (US1: Reduction) — depends on Phase 2 complete
  T010 → T012 → T013
  T010 → T014 → T015
  T013 → T014
  T014 → T016 → T017
  T017 → T057 → T058 (target mapping, conditional)
  T058 → T059
  T017 → T018 (if binary target, skip T057-T059)
  T018 → T019
  T019 → T020 → T021
  T021 → T022, T023, T024, T025  (sub-dialogs, parallelizable)
  T021 → T026 → T027
  T011 → T028
  T027 → T028
  T025 → T060

Phase 4 (US2: Conversion) — depends on Phase 2 complete, independent of Phase 3
  T010 → T029 → T030
  T010 → T031 → T032
  T030 → T033 → T034

Phase 5 (US3: CLI Integration) — depends on Phase 2 (T007 entry point)
  T007 → T035
  T035 → T036
  T035 → T037
  T028 → T037 (needs WizardData::to_config)
  T034 → T038 (needs conversion result handling)
  T037 → T038
  T038 → T039
  T035 → T040

Phase 6 (US4: Testing) — can start after Phase 2 for state machine tests
  T005 → T041, T042, T043
  T011 → T044, T045, T046
  T028 → T047, T048, T049
  T035 → T050, T051, T052
  T057 → T061, T062, T063, T064
  T009 → T065, T066

Phase 7 (US5: Documentation) — should happen last
  T039 → T053, T054, T055
  T056 depends on all wizard steps being implemented (T013-T034)
```

### Simplified Phase Dependency Order

```
Phase 1 (Setup)
    ↓
Phase 2 (Foundational)
    ↓
  ┌─────────────┬──────────────┬─────────────────┐
  ↓             ↓              ↓                 ↓
Phase 3       Phase 4      Phase 5 (partial)  Phase 6 (partial)
(US1)         (US2)        (--manual flag)     (state tests)
  │             │              │                 │
  └──────┬──────┘              │                 │
         ↓                     ↓                 │
    Phase 5 (full)        Phase 6 (full)         │
    (orchestration)       (integration tests)    │
         │                     │                 │
         └──────────┬──────────┘─────────────────┘
                    ↓
              Phase 7 (Documentation)
```

## Parallel Execution Opportunities

### Within Phase 2 (Foundational)
- T009 (rendering framework) and T011 (validation) can be done in parallel after T003

### Phase 3 + Phase 4 (US1 + US2)
- Reduction path (T012-T028) and Conversion path (T029-T034) are fully independent and can be implemented in parallel

### Within Phase 3 (US1: Optional Settings)
- T022 (solver dialog), T023 (weight dialog), T024 (drop columns dialog), T025 (schema dialog) are independent sub-dialogs that can be implemented in parallel

### Phase 6 (Testing)
- T041-T043 (state machine tests), T044-T046 (validation tests), T050-T052 (CLI tests) can all be written in parallel after their dependencies complete

## Implementation Strategy

### MVP Scope (User Story 1 + CLI Integration)
The minimum viable product is:
1. **Phase 1** (Setup) + **Phase 2** (Foundational) — Core infrastructure
2. **Phase 3** (US1: Reduction path) — The primary user flow
3. **Phase 5** (US3: CLI integration) — Wire wizard into main.rs
4. **Phase 6** (US4: Core tests) — State machine + validation tests

This gives a functional wizard for the reduction pipeline, which is the primary use case.

### Incremental Additions
After MVP:
- **Phase 4** (US2: Conversion path) — Adds conversion wizard
- **Phase 7** (US5: Documentation) — Polishes help text and docs
- **Phase 6 remaining** (US4: Integration tests) — Full test coverage

### Key Risks to Monitor
- Terminal teardown must be guaranteed even on panic (T006)
- `WizardData::to_config()` must produce identical `Config` to dashboard mode (T028)
- `--manual` flag must not change existing dashboard behavior (T037)

---

## Summary

| Metric | Count |
|--------|-------|
| **Total Tasks** | 68 |
| **Phase 1 (Setup)** | 2 |
| **Phase 2 (Foundational)** | 11 |
| **Phase 3 (US1: Reduction)** | 21 |
| **Phase 4 (US2: Conversion)** | 6 |
| **Phase 5 (US3: CLI Integration)** | 6 |
| **Phase 6 (US4: Testing)** | 18 |
| **Phase 7 (US5: Documentation)** | 4 |
| **Parallelizable Tasks** | 9 explicitly marked [P], plus cross-phase parallelism |

---

## Completion Checklist

- [x] All tasks marked Done
- [x] CI passes (`cargo clippy --all-targets --all-features -- -D warnings` + `cargo fmt -- --check` + `cargo test --all-features`)
- [x] Manual TUI testing completed (wizard + dashboard + CLI modes)
- [x] CLAUDE.md updated with wizard architecture
- [x] Constitution compliance verified (Principles 3, 4, 5)
