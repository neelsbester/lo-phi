# Feature Specification: TUI Wizard Mode

**Version:** 0.1.0
**Date:** 2026-02-02
**Status:** Draft

---

## Summary

Lo-phi currently launches into a configuration dashboard where all settings are visible at once. While powerful for experienced users, this can be overwhelming for new users or infrequent users who may not know which parameters matter for their task. This feature introduces a **wizard mode** that guides users through the tool step-by-step, asking one question at a time in a logical sequence. The wizard becomes the default experience, while the existing dashboard-style menu is preserved as "expert mode" for users who prefer it.

## Constitution Alignment

| Principle                        | Applicable | Notes                                                                                    |
| -------------------------------- | ---------- | ---------------------------------------------------------------------------------------- |
| 1. Statistical Correctness       | No         | No change to analysis logic or outputs                                                   |
| 2. Performance at Scale          | Partial    | CSV-to-Parquet conversion wizard path satisfies P2 convenience mandate. NFR-2 (transitions < 100ms) addresses UI responsiveness. Core pipeline execution is unchanged. |
| 3. Transparent Decision-Making   | Yes        | Wizard should clearly communicate what each setting controls and what defaults are used   |
| 4. Ergonomic TUI/CLI             | Yes        | Primary motivation: lower the barrier to entry with guided configuration                 |
| 5. Rigorous Testing              | Yes        | Wizard flow needs integration tests covering the full path from launch to pipeline start |

## Clarifications

### Session 2026-02-02

- Q: What should the wizard step order be for the reduction path? → A: Task → File → Target → Thresholds (missing, Gini, correlation) → Optional settings → Summary
- Q: Should there be an interactive mode selection screen at launch, or should mode be controlled solely via CLI flags? → A: No mode selection screen. Wizard starts by default. `--manual` flag launches the existing dashboard (expert mode) directly.
- Q: How should the wizard handle invalid input at threshold/numeric steps? → A: Show inline error message on the same step (e.g., "Value must be between 0.0 and 1.0") and let the user re-enter. The wizard does not advance until a valid value is provided.

## Requirements

### Functional Requirements

1. **FR-1: Wizard as Default Launch Mode** — When the user launches Lo-phi without the `--no-confirm` or `--manual` flags, the wizard starts directly with no intermediate mode selection screen.

2. **FR-2: Manual Mode Flag** — See FR-11 for full mode selection behavior. The `--manual` CLI flag launches the existing dashboard-style configuration menu, bypassing the wizard.

3. **FR-3: Task Selection Step** — The wizard's first step asks the user what they want to do. The available tasks are:
   - **Feature Reduction** — the core analysis pipeline (missing, Gini/IV, correlation)
   - **CSV to Parquet Conversion** — convert a CSV file to Parquet format

4. **FR-4: File Selection Step** — After task selection, the wizard prompts the user to select an input file. If a file was already specified via `--input`, the interactive file picker is skipped; the wizard displays a non-interactive banner confirming which file will be used and proceeds to the next step automatically.

5. **FR-5: Target Column Selection** — For the reduction task, after file selection, the wizard prompts the user to select a target column from the dataset's columns. This step includes the existing search/filter functionality. Target selection must precede threshold configuration because the dataset must be loaded first.

6. **FR-6: Guided Threshold Configuration** — For the reduction task, after target selection, the wizard walks the user through each threshold one at a time (missing, Gini, correlation), displaying the default value and a brief explanation of what the threshold controls. The user can accept the default or enter a custom value.

7. **FR-7: Optional Settings** — After the core settings, the wizard asks whether the user wants to configure optional settings (solver options, weight column, columns to drop, schema inference length). If the user declines, sensible defaults are used. If the user accepts, each optional setting is presented one at a time.

8. **FR-8: Confirmation Summary** — Before execution, the wizard displays a summary of all selected settings and asks the user to confirm, go back to adjust a setting, or cancel.

9. **FR-9: Conversion Wizard Path** — When the user selects CSV-to-Parquet conversion, the wizard guides them through: file selection, output path confirmation, and conversion mode choice (streaming vs fast in-memory). The default conversion mode is fast (in-memory).

10. **FR-10: Expert Mode Preservation** — The existing dashboard-style configuration menu remains fully functional and accessible. No features are removed from the current TUI experience.

11. **FR-11: Mode Selection Behavior** — Lo-phi supports three mutually exclusive launch modes determined by CLI flags:
    - **Default (no flags):** Wizard mode starts, guiding the user step-by-step.
    - **`--manual`:** The existing dashboard-style configuration menu launches directly, bypassing the wizard.
    - **`--no-confirm`:** All interactive UI is skipped; pipeline runs from CLI arguments alone.
    When both `--manual` and `--no-confirm` are provided, `--no-confirm` takes precedence (no interactive UI at all).

12. **FR-12: Back Navigation** — At any step in the wizard, the user can go back to the previous step to change their selection without restarting the entire wizard.

13. **FR-13: Progress Indication** — The wizard displays a progress indicator showing the current step and total steps (e.g., "Step 3 of 6") so the user knows how far along they are in the configuration process.

14. **FR-14: Inline Input Validation** — When the user enters an invalid value at any wizard step (e.g., non-numeric text for a threshold, a value outside the valid range), the wizard displays an inline error message explaining the constraint (e.g., "Value must be between 0.0 and 1.0") and remains on the same step until a valid value is provided.

### Non-Functional Requirements

1. **NFR-1: Discoverability** — Each wizard step must include a help description of no more than two lines that states (a) the parameter's default value, (b) the valid range, and (c) a one-sentence explanation of the parameter's impact. Users must be able to understand the impact of their choices without needing external documentation.

2. **NFR-2: Responsiveness** — Wizard step transitions (forward and backward navigation) must complete in under 100ms, ensuring no perceptible delay. File and column loading operations may take longer but must display a loading indicator if they exceed 200ms.

3. **NFR-3: Keyboard-Driven** — The wizard must be fully navigable using only the keyboard, consistent with the existing TUI interaction patterns.

4. **NFR-4: Terminal Compatibility** — The wizard must render correctly in terminals that are at least 80 columns wide and 24 rows tall, matching the existing TUI requirements.

## Scope

### In Scope

- Wizard flow for the feature reduction pipeline (file, target, thresholds, optional settings)
- Wizard flow for CSV-to-Parquet conversion (file, output, mode)
- `--manual` CLI flag to launch the existing dashboard directly
- Back navigation within wizard steps
- Step progress indicator
- Confirmation summary before execution
- Brief inline help text for each setting

### Out of Scope

- Changes to the analysis pipeline logic
- Changes to the existing expert mode dashboard layout or keyboard shortcuts
- Persisting user preferences between sessions (e.g., remembering last-used mode)
- Multi-language or localization support
- Mouse interaction support
- Wizard flows for any future analysis types not yet implemented

## Assumptions

- The wizard mode targets the same user persona as the existing TUI: data scientists comfortable with terminal applications but not necessarily familiar with Lo-phi's specific parameters.
- The wizard presents the same configurable parameters as the current TUI dashboard — no new parameters are introduced by this feature.
- Default values for all settings remain unchanged from current defaults (missing: 0.30, Gini: 0.05, correlation: 0.40, solver: on, trend: none, schema inference: 10000).
- The wizard reuses the existing column selection UI (with search/filter) rather than introducing a new selection pattern.
- CLI-only parameters (binning strategy, pre-bins, cart-min-bin-pct, min-category-samples, solver-timeout, solver-gap) remain CLI-only and are not exposed in the wizard, consistent with the current expert mode.

## User Scenarios & Testing

### Scenario 1: First-Time User — Reduction Pipeline

**Given** a user launches Lo-phi for the first time without any CLI flags
**When** the tool starts
**Then** the wizard mode begins and asks what task they want to perform
**When** the user selects "Feature Reduction"
**Then** the wizard prompts for file selection
**When** the user selects a CSV file
**Then** the wizard prompts for target column selection
**When** the user selects a target column
**Then** the wizard walks through each threshold (missing, Gini, correlation) one at a time, showing defaults
**When** the user accepts all defaults
**Then** the wizard asks if the user wants to configure optional settings
**When** the user declines
**Then** a confirmation summary is displayed with all settings
**When** the user confirms
**Then** the pipeline begins execution

### Scenario 2: Experienced User — Expert Mode via Flag

**Given** a user who prefers the dashboard layout
**When** they launch Lo-phi with the `--manual` flag
**Then** the existing dashboard-style configuration menu appears immediately, with no wizard interaction

### Scenario 3: Wizard — CSV to Parquet Conversion

**Given** a user launches Lo-phi in wizard mode
**When** they select "CSV to Parquet Conversion" as the task
**Then** the wizard prompts for input file selection
**When** the user selects a CSV file
**Then** the wizard asks for output path confirmation (with a sensible default)
**When** the user accepts
**Then** the wizard asks which conversion mode to use (streaming or fast)
**When** the user selects a mode
**Then** conversion begins

### Scenario 4: Wizard — Back Navigation

**Given** a user is on step 4 (correlation threshold) of the wizard
**When** they press the back key
**Then** they return to step 3 (Gini threshold) with their previously entered value preserved
**When** they press back again
**Then** they return to step 2 (missing threshold) with their previously entered value preserved

### Scenario 5: Wizard — Input File Pre-specified via CLI

**Given** a user launches Lo-phi with `--input data.csv` (no mode flag)
**When** the wizard starts
**Then** the file selection step is skipped
**And** the wizard displays a confirmation that `data.csv` will be used
**And** proceeds to the next applicable step

### Scenario 6: Non-Binary Target Handling in Wizard

**Given** a user selects a target column
**When** the wizard proceeds past target selection
**Then** the wizard always presents the TargetMapping step, showing the unique target values and asking the user to select the event value and non-event value
**When** the target is already binary (0/1), the wizard pre-selects the mapping (1=event, 0=non-event) and the user can accept with Enter or override
**And** continues the wizard flow after mapping is complete

**Design Note:** The TargetMapping step is always shown rather than conditionally inserted. This avoids the need to load data values during file selection (the wizard only reads column schema). It also makes the step count predictable for the progress indicator and simplifies back navigation.

### Scenario 7: Wizard — Cancel at Any Point

**Given** a user is at any step in the wizard
**When** they press Escape or the quit key
**Then** a confirmation prompt asks if they want to exit
**When** they confirm
**Then** Lo-phi exits cleanly without running any analysis

## Success Criteria

- New users can complete a full reduction pipeline configuration through the wizard without consulting documentation
- Experienced users can bypass the wizard entirely using `--manual` and land in the familiar dashboard with zero additional keystrokes
- All wizard steps are completable using keyboard-only navigation
- The wizard completes the configuration flow (from launch to pipeline start) with no more than 8 user interactions for a typical reduction task using default settings
- Back navigation preserves all previously entered values with 100% accuracy
- The existing expert mode remains fully functional with no behavioral changes
- The `--no-confirm` flag continues to work as before, bypassing all interactive UI

## Acceptance Criteria

- [ ] Launching Lo-phi without flags starts in wizard mode by default
- [ ] Wizard presents task selection (Reduction vs Conversion) as the first step
- [ ] Wizard walks through file selection, target selection, and thresholds sequentially
- [ ] Each wizard step displays inline help text explaining the setting
- [ ] A step progress indicator is visible at each wizard step
- [ ] Back navigation works at every step and preserves entered values
- [ ] A confirmation summary screen shows all settings before execution
- [ ] `--manual` flag launches the existing dashboard directly
- [ ] Dashboard mode (current behavior) is fully preserved and unchanged
- [ ] `--no-confirm` continues to bypass all interactive UI
- [ ] Non-binary target columns trigger event/non-event mapping steps within the wizard
- [ ] CSV-to-Parquet conversion has its own wizard path
- [ ] Escape/quit at any step prompts for confirmation before exiting
- [ ] Invalid input at numeric steps shows inline error and stays on the same step

## Keybinding Reference

The following keybindings apply consistently across all wizard steps:

| Key | Action | Notes |
|-----|--------|-------|
| `Enter` | Advance to next step | Only if current step passes validation |
| `Backspace` | Go back to previous step | Preserves all entered values; disabled at first step |
| `Esc` | Show quit confirmation dialog | Displays "Are you sure?" overlay; does not quit immediately |
| `Q` | Show quit confirmation dialog | Alias for Esc (quit confirmation) |
| `Up` / `Down` | Navigate lists | For steps with selectable options |
| `Space` | Toggle checkbox | For multi-select steps (e.g., drop columns) |
| Typing | Edit text / filter lists | Context-dependent per step |

**Note:** `Esc` always triggers the quit confirmation dialog, never "go back." Use `Backspace` for back navigation.
