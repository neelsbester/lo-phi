# TUI Wizard Mode - Data Model Specification

## Overview

This document defines the data structures, state transitions, validation rules, and relationships for the step-by-step wizard mode in Lo-phi's TUI. The wizard guides users through pipeline configuration for either feature reduction or CSV-to-Parquet conversion.

## Core Entities

### 1. WizardTask (Enum)

Represents the high-level task the user wants to perform.

```rust
pub enum WizardTask {
    /// Core feature reduction pipeline (missing → Gini/IV → correlation)
    Reduction,

    /// CSV to Parquet conversion
    Conversion,
}
```

**Purpose:** Determines which sequence of steps to present to the user.

---

### 2. WizardStep (Enum)

Represents each individual step in the wizard flow. Steps are either shared between tasks or task-specific.

```rust
pub enum WizardStep {
    // ============ SHARED STEPS ============

    /// Initial task selection (Reduction or Conversion)
    TaskSelection,

    /// File picker for input dataset
    FileSelection,

    // ============ REDUCTION-SPECIFIC STEPS ============

    /// Select target column with search/filter
    TargetSelection {
        search: String,
        columns: Vec<String>,
        filtered: Vec<usize>,  // Indices into columns matching search
        selected: usize,        // Index into filtered list
    },

    /// Select event and non-event values for non-binary target columns
    TargetMapping {
        unique_values: Vec<String>,  // Unique values from target column
        event_selected: Option<usize>,      // Index of selected event value
        non_event_selected: Option<usize>,  // Index of selected non-event value
        focus: TargetMappingFocus,   // Which field has focus (Event or NonEvent)
    },

    /// Configure missing value threshold (0.0-1.0)
    MissingThreshold {
        input: String,
        error: Option<String>,
    },

    /// Configure Gini/IV threshold (0.0-1.0)
    GiniThreshold {
        input: String,
        error: Option<String>,
    },

    /// Configure correlation threshold (0.0-1.0)
    CorrelationThreshold {
        input: String,
        error: Option<String>,
    },

    /// Prompt asking if user wants to configure optional settings
    OptionalSettingsPrompt,

    /// Toggle solver usage (on/off)
    SolverToggle {
        selected: bool,
    },

    /// Select monotonicity constraint
    MonotonicitySelection {
        selected: usize,  // Index into ["none", "ascending", "descending", "peak", "valley", "auto"]
    },

    /// Select weight column (optional) with search/filter
    WeightColumn {
        search: String,
        columns: Vec<String>,
        filtered: Vec<usize>,
        selected: usize,
    },

    /// Multi-select columns to drop with search/filter
    DropColumns {
        search: String,
        columns: Vec<String>,
        filtered: Vec<usize>,
        selected: usize,
        checked: Vec<bool>,  // Length matches columns
    },

    /// Configure schema inference row count
    SchemaInference {
        input: String,
        error: Option<String>,
    },

    /// Review all settings before execution (shared by both reduction and conversion paths)
    Summary,

    // ============ CONVERSION-SPECIFIC STEPS ============

    /// Enter output .parquet file path
    OutputPath {
        input: String,
        error: Option<String>,
    },

    /// Select conversion mode (streaming or fast in-memory)
    ConversionMode {
        selected: usize,  // 0 = streaming, 1 = fast (defaults to 1 to match WizardData.conversion_fast = true)
    },
}
```

**Step Categories:**
- **Shared:** TaskSelection, FileSelection
- **Required Reduction:** TargetSelection, TargetMapping (always shown; pre-selects 1/0 for binary targets), MissingThreshold, GiniThreshold, CorrelationThreshold, Summary
- **Optional Reduction:** SolverToggle, MonotonicitySelection, WeightColumn, DropColumns, SchemaInference
- **Conversion:** OutputPath, ConversionMode

---

### 3. WizardData (Struct)

Accumulates user configuration as they progress through wizard steps.

```rust
pub struct WizardData {
    // ============ TASK CONTEXT ============

    /// Selected task (None until TaskSelection completes)
    pub task: Option<WizardTask>,

    /// Path to input dataset (CSV or Parquet)
    pub input_path: Option<PathBuf>,

    /// Column names loaded from input file
    pub columns: Vec<String>,

    // ============ REDUCTION PARAMETERS ============

    /// Target column name for binary classification
    pub target: Option<String>,

    /// Threshold for dropping features with too many missing values (0.0-1.0)
    pub missing_threshold: f64,  // Default: 0.30

    /// Minimum Gini coefficient to retain feature (0.0-1.0)
    pub gini_threshold: f64,  // Default: 0.05

    /// Maximum correlation between features before dropping one (0.0-1.0)
    pub correlation_threshold: f64,  // Default: 0.40

    /// Whether to use MILP solver for optimal binning
    pub use_solver: bool,  // Default: true

    /// Monotonicity constraint for WoE ("none", "ascending", "descending", "peak", "valley", "auto")
    pub monotonicity: String,  // Default: "none"

    /// Optional weight column for weighted Gini/IV calculation
    pub weight_column: Option<String>,

    /// Columns to drop before analysis (user-specified exclusions)
    pub drop_columns: Vec<String>,

    /// Target mapping for non-binary targets (event/non-event values)
    pub target_mapping: Option<TargetMapping>,

    /// Number of rows to scan for schema inference (0 = full scan)
    pub infer_schema_length: usize,  // Default: 10000

    // ============ CONVERSION PARAMETERS ============

    /// Output path for converted Parquet file
    pub output_path: Option<PathBuf>,

    /// Use fast in-memory conversion (vs streaming)
    pub conversion_fast: bool,  // Default: true
}
```

**Field Initialization:**
- Fields with defaults are initialized in `WizardData::default()`
- `Option<T>` fields start as `None` and are populated during wizard flow
- `columns` is populated after `FileSelection` completes via Polars schema read

**Defaults Alignment:**
These defaults match the existing `Config` struct and CLI defaults to ensure consistent behavior.

---

### 4. WizardState (Struct)

The overall state machine managing wizard progression.

```rust
pub struct WizardState {
    /// Ordered sequence of steps to execute
    pub steps: Vec<WizardStep>,

    /// Current position in steps vector
    pub current_index: usize,

    /// Accumulated configuration data
    pub data: WizardData,

    /// Whether to show quit confirmation dialog
    pub show_quit_confirm: bool,
}
```

**State Management:**
- `steps` is dynamically populated based on user choices (e.g., optional settings prompt)
- `current_index` advances on successful step completion, decrements on back navigation
- `show_quit_confirm` is toggled by Esc key, requires confirmation before quitting

**Lifecycle:**
1. Initialize with `[TaskSelection]` as the only step
2. User selects task → append task-specific steps
3. User navigates forward/backward → modify `current_index`
4. User completes Summary → convert `data` to `WizardResult`

**Back Navigation and State Preservation:**
- Step instances are kept in the `steps` vector and retain their embedded state (search strings, input values, selected indices)
- When `current_index` decrements (back navigation), the step at that index still holds the user's previous input
- `build_steps()` only rebuilds the step sequence after task selection (replacing future steps). It does not modify steps at indices <= `current_index`
- Optional steps (inserted after `OptionalSettingsPrompt` "Yes") are appended to the end; pressing back from them returns to the prompt
- When a step is revisited, its state is displayed as the user left it, ensuring FR-12 (back navigation preserves values) is satisfied

---

### 5. WizardResult (Enum)

Return value from wizard mode to main pipeline orchestrator.

```rust
pub enum WizardResult {
    /// Run feature reduction pipeline with this config
    RunReduction(Box<Config>),

    /// Run CSV-to-Parquet conversion with this config
    RunConversion(Box<ConversionConfig>),

    /// User quit without completing wizard
    Quit,
}
```

/// Conversion-specific configuration (separate from Config since
/// Config lacks conversion fields like `fast`)
pub struct ConversionConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub infer_schema_length: usize,
    pub fast: bool,
}
```

**Mapping to Existing Types:**
- `RunReduction(config)` → `ConfigResult::Proceed(config)` — uses existing `Config` struct
- `RunConversion(conversion_config)` → directly calls `run_convert()` — uses `ConversionConfig` struct (not `Config`, which lacks conversion-specific fields like `fast`)
- `Quit` → `ConfigResult::Quit`

The `Config` struct is constructed from `WizardData` with CLI defaults for non-wizard parameters. `ConversionConfig` is constructed directly from `WizardData` conversion fields.

---

## State Transition Diagram

```
                           ┌──────────────────┐
                           │ TaskSelection    │
                           └────┬────────┬────┘
                                │        │
                    Reduction   │        │   Conversion
                                │        │
                    ┌───────────▼─┐   ┌─▼──────────────┐
                    │FileSelection│   │FileSelection   │
                    └───────┬─────┘   └─┬──────────────┘
                            │            │
                  Load      │            │
                  columns   │            │
                            │            │
                ┌───────────▼─────┐      │
                │TargetSelection  │      │
                └───────┬─────────┘      │
                        │                │
               ┌────────▼───────┐        │
               │TargetMapping   │        │
               │(always shown)  │        │
               └────────┬───────┘        │
                        │                │
              ┌─────────▼─────────┐      │
              │MissingThreshold   │      │
              └─────────┬─────────┘      │
                        │                │
              ┌─────────▼─────────┐      │
              │GiniThreshold      │      │
              └─────────┬─────────┘      │
                        │                │
          ┌─────────────▼───────────────┐│
          │CorrelationThreshold         ││
          └─────────────┬───────────────┘│
                        │                │
       ┌────────────────▼──────────────┐ │
       │OptionalSettingsPrompt         │ │
       └────┬────────────────────┬─────┘ │
            │                    │       │
          Yes│                   │No     │
            │                    │       │
   ┌────────▼────────┐           │       │
   │SolverToggle     │           │       │
   └────────┬────────┘           │       │
            │                    │       │
   ┌────────▼─────────────┐      │       │
   │MonotonicitySelection │      │       │
   └────────┬─────────────┘      │       │
            │                    │       │
   ┌────────▼────────┐           │       │
   │WeightColumn     │           │       │
   └────────┬────────┘           │       │
            │                    │       │
   ┌────────▼────────┐           │       │
   │DropColumns      │           │       │
   └────────┬────────┘           │       │
            │                    │       │
   ┌────────▼────────┐           │       │
   │SchemaInference  │           │       │
   └────────┬────────┘           │       │
            │                    │       │
            └────────┬───────────┘       │
                     │                   │
            ┌────────▼────────┐  ┌───────▼──────────┐
            │Summary          │  │OutputPath        │
            └────────┬────────┘  └───────┬──────────┘
                     │                   │
          ┌──────────▼──────┐   ┌────────▼──────────┐
          │Confirm → Result │   │ConversionMode     │
          └─────────────────┘   └────────┬──────────┘
                                          │
                                 ┌────────▼────────┐
                                 │Summary          │
                                 └────────┬────────┘
                                          │
                                 ┌────────▼──────┐
                                 │Confirm→Result │
                                 └───────────────┘

Navigation:
- Enter/Right: Advance to next step (if valid)
- Backspace/Left: Go back to previous step
- Esc: Show quit confirmation dialog
- At Summary: Confirm → WizardResult, Back → previous step
```

**Key Transitions:**

| From Step | Event | To Step | Side Effects |
|-----------|-------|---------|--------------|
| TaskSelection | Select Reduction | FileSelection | Append reduction steps to `steps` |
| TaskSelection | Select Conversion | FileSelection | Append conversion steps to `steps` |
| FileSelection | File selected | TargetSelection (Reduction) or OutputPath (Conversion) | Load column names into `data.columns` |
| TargetSelection | Select column | TargetMapping | Store `data.target`; load unique target values for mapping step |
| TargetMapping | Select event + non-event values | MissingThreshold | Store `data.target_mapping`; for binary targets, pre-select 1=event/0=non-event (user can override or accept with Enter) |
| MissingThreshold | Valid input | GiniThreshold | Parse and store `data.missing_threshold` |
| MissingThreshold | Invalid input | MissingThreshold | Set `error` field in step |
| GiniThreshold | Valid input | CorrelationThreshold | Parse and store `data.gini_threshold` |
| CorrelationThreshold | Valid input | OptionalSettingsPrompt | Parse and store `data.correlation_threshold` |
| OptionalSettingsPrompt | Select "Yes" | SolverToggle | Insert optional steps after current step |
| OptionalSettingsPrompt | Select "No" | Summary | Skip optional steps |
| SolverToggle | Toggle | MonotonicitySelection | Store `data.use_solver` |
| MonotonicitySelection | Select option | WeightColumn | Store `data.monotonicity` |
| WeightColumn | Select column or None | DropColumns | Store `data.weight_column` |
| DropColumns | Confirm selection | SchemaInference | Store `data.drop_columns` |
| SchemaInference | Valid input | Summary | Parse and store `data.infer_schema_length` |
| OutputPath | Enter path | ConversionMode | Store `data.output_path` |
| ConversionMode | Select mode | Summary | Store `data.conversion_fast` |
| Summary | Confirm | (Exit) | Return `WizardResult::RunReduction` or `RunConversion` |
| Any step | Backspace/Left | Previous step | `current_index -= 1` (if > 0) |
| Any step | Esc | (Show dialog) | `show_quit_confirm = true` |

---

## Validation Rules

### Field-Level Validation

| Field | Rules | Error Message |
|-------|-------|---------------|
| `missing_threshold` | Must parse as `f64` | "Invalid number format" |
| | `0.0 <= value <= 1.0` | "Threshold must be between 0.0 and 1.0" |
| `gini_threshold` | Must parse as `f64` | "Invalid number format" |
| | `0.0 <= value <= 1.0` | "Threshold must be between 0.0 and 1.0" |
| `correlation_threshold` | Must parse as `f64` | "Invalid number format" |
| | `0.0 <= value <= 1.0` | "Threshold must be between 0.0 and 1.0" |
| `infer_schema_length` | Must parse as `usize` | "Invalid number format" |
| | `value >= 100` or `value == 0` | "Must be 0 (full scan) or >= 100 rows" |
| `target` | Must exist in `data.columns` | "Column not found in dataset" |
| | Cannot be empty string | "Target column required" |
| `target_mapping.event` | Must differ from non-event value | "Event and non-event values must be different" |
| `target_mapping.non_event` | Must differ from event value | "Event and non-event values must be different" |
| `input_path` | File must exist | "File not found" |
| | Extension must be `.csv` or `.parquet` | "Only CSV and Parquet files supported" |
| `output_path` | Extension must be `.parquet` | "Output must have .parquet extension" |
| | Parent directory must exist | "Directory does not exist" |
| `weight_column` | If Some, must exist in `data.columns` | "Column not found in dataset" |
| `drop_columns` | All entries must exist in `data.columns` | "Unknown column: {name}" |

### Step-Level Validation

Steps cannot advance unless their validation passes:

| Step | Validation Check |
|------|------------------|
| TaskSelection | User must select a task |
| FileSelection | File must be selected and loadable |
| TargetSelection | Column must be selected |
| TargetMapping | Event and non-event values must be selected and differ |
| MissingThreshold | Input must pass field validation |
| GiniThreshold | Input must pass field validation |
| CorrelationThreshold | Input must pass field validation |
| SchemaInference | Input must pass field validation |
| OutputPath | Path must pass field validation |
| ConversionMode | User must select a mode |
| Summary | All required fields in `WizardData` must be populated |

**Summary Step Required Fields:**

For Reduction:
- `task` (must be `Some(Reduction)`)
- `input_path` (must be `Some`)
- `target` (must be `Some`)

For Conversion:
- `task` (must be `Some(Conversion)`)
- `input_path` (must be `Some`)
- `output_path` (must be `Some`)

---

## Mapping: WizardData → Config

When the wizard completes, `WizardData` is converted to the existing `Config` struct. Non-wizard fields receive defaults from the `Cli` struct.

### Direct Mappings

| WizardData Field | Config Field | Notes |
|------------------|--------------|-------|
| `input_path` | `input` | Unwrap from Option |
| `target` | `target` | Unwrap from Option |
| `missing_threshold` | `missing_threshold` | Direct copy |
| `gini_threshold` | `gini_threshold` | Direct copy |
| `correlation_threshold` | `correlation_threshold` | Direct copy |
| `use_solver` | `use_solver` | Direct copy |
| `monotonicity` | `monotonicity` | Direct copy (String) |
| `weight_column` | `weight_column` | Direct copy (Option) |
| `drop_columns` | `drop_columns` | Direct copy (Vec) |
| `infer_schema_length` | `infer_schema_length` | Direct copy |
| `output_path` | `output` | For conversion, unwrap from Option; for reduction, derive from input |

### CLI-Default Mappings

Fields not configurable in wizard mode use defaults from `Cli::default()`:

| Config Field | Source | Default Value |
|--------------|--------|---------------|
| `gini_bins` | `Cli::gini_bins` | 10 |
| `binning_strategy` | `Cli::binning_strategy` | `BinningStrategy::Cart` |
| `prebins` | `Cli::prebins` | 20 |
| `cart_min_bin_pct` | `Cli::cart_min_bin_pct` | 5.0 |
| `min_category_samples` | `Cli::min_category_samples` | 5 |
| `solver_timeout` | `Cli::solver_timeout` | 30 seconds |
| `solver_gap` | `Cli::solver_gap` | 0.01 |
| `event_value` | `Cli::event_value` | None (auto-detect) |
| `non_event_value` | `Cli::non_event_value` | None (auto-detect) |

### Conversion Logic

```rust
impl WizardData {
    /// Convert wizard data to the appropriate result type
    pub fn to_result(&self) -> Result<WizardResult> {
        let cli_defaults = Cli::default();

        match self.task {
            Some(WizardTask::Reduction) => {
                let input = self.input_path.as_ref().ok_or("Missing input path")?;
                // Derive output path: input.csv → input_reduced.csv
                let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
                let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("csv");
                let output = input.with_file_name(format!("{stem}_reduced.{ext}"));

                Ok(WizardResult::RunReduction(Box::new(Config {
                    input: input.clone(),
                    output,
                    target: self.target.clone().ok_or("Missing target")?,
                    weight_column: self.weight_column.clone(),
                    drop_columns: self.drop_columns.clone(),
                    missing_threshold: self.missing_threshold,
                    gini_threshold: self.gini_threshold,
                    correlation_threshold: self.correlation_threshold,
                    gini_bins: cli_defaults.gini_bins,
                    binning_strategy: cli_defaults.binning_strategy,
                    prebins: cli_defaults.prebins,
                    use_solver: self.use_solver,
                    monotonicity: self.monotonicity.clone(),
                    solver_timeout: cli_defaults.solver_timeout,
                    solver_gap: cli_defaults.solver_gap,
                    min_category_samples: cli_defaults.min_category_samples,
                    cart_min_bin_pct: cli_defaults.cart_min_bin_pct,
                    event_value: cli_defaults.event_value,
                    non_event_value: cli_defaults.non_event_value,
                    infer_schema_length: self.infer_schema_length,
                })))
            }
            Some(WizardTask::Conversion) => {
                let input = self.input_path.as_ref().ok_or("Missing input path")?;
                let output = self.output_path.as_ref().ok_or("Missing output path")?;

                Ok(WizardResult::RunConversion(Box::new(ConversionConfig {
                    input: input.clone(),
                    output: output.clone(),
                    infer_schema_length: self.infer_schema_length,
                    fast: self.conversion_fast,
                })))
            }
            None => Err("No task selected"),
        }
    }
}
```

---

## Step Data Structures

### Search/Filter Pattern

Several steps use a common pattern for searchable lists (target, weight, drop columns):

```rust
pub struct SearchableList {
    /// User's search input
    pub search: String,

    /// Full list of available items
    pub columns: Vec<String>,

    /// Indices into `columns` that match `search` (case-insensitive substring)
    pub filtered: Vec<usize>,

    /// Currently highlighted index in `filtered` list (0-based)
    pub selected: usize,
}
```

**Filtering Logic:**
- If `search` is empty, `filtered` contains all indices `[0, 1, 2, ..., columns.len()-1]`
- Otherwise, `filtered` contains indices where `columns[i].to_lowercase().contains(&search.to_lowercase())`
- `selected` is clamped to `filtered.len().saturating_sub(1)` when filter changes

**Navigation:**
- Up/Down keys adjust `selected`
- Typing updates `search` and recomputes `filtered`
- Enter confirms selection of `columns[filtered[selected]]`

### Multi-Select Pattern (DropColumns)

Extends the searchable list with checkboxes:

```rust
pub struct MultiSelectList {
    // ... all SearchableList fields ...

    /// Checked state for each item in `columns` (parallel array)
    pub checked: Vec<bool>,
}
```

**Interaction:**
- Space bar toggles `checked[filtered[selected]]`
- Enter confirms selection (returns all indices where `checked[i] == true`)

---

## Error Handling

### Step-Level Errors

Steps with `error: Option<String>` field (threshold inputs, schema inference) display inline validation errors:

```
┌─────────────────────────────────────────┐
│ Missing Value Threshold                 │
├─────────────────────────────────────────┤
│                                         │
│ Enter threshold (0.0-1.0):              │
│ > 1.5_                                  │
│                                         │
│ ✗ Threshold must be between 0.0 and 1.0│
│                                         │
│ [Enter] Continue  [Esc] Quit            │
└─────────────────────────────────────────┘
```

Errors are cleared on next input change.

### File Loading Errors

If `FileSelection` fails to load column schema:

1. Display error toast: "Failed to load file: {error}"
2. Stay on FileSelection step
3. User can select a different file or quit

### Conversion Errors

If `WizardData::to_config()` fails (missing required fields):

1. Log error (should never happen if Summary validation passes)
2. Return `WizardResult::Quit` as fallback

---

## Memory and Performance

### Column Loading Strategy

After `FileSelection`:
```rust
let schema = LazyCsvReader::new(&path)
    .with_infer_schema_length(Some(data.infer_schema_length))
    .finish()?
    .schema()?;

data.columns = schema.iter_names().map(|s| s.to_string()).collect();
```

- Only reads schema, not full dataset
- Uses user-configured `infer_schema_length` (default 10000 rows)
- Columns vector is kept in memory throughout wizard (typically <100 items, ~few KB)

### Search Filtering

Search filters are recomputed on every keystroke:
```rust
filtered = columns.iter()
    .enumerate()
    .filter(|(_, col)| col.to_lowercase().contains(&search.to_lowercase()))
    .map(|(i, _)| i)
    .collect();
```

- O(n*m) where n = column count, m = average column name length
- Acceptable for typical datasets (<1000 columns)
- Pre-lowercase column names if performance becomes issue

---

## Future Extensions

### Planned Enhancements

1. **Preset Profiles:** Save/load wizard configurations as named presets
   - Add new variant: `WizardStep::LoadPreset { presets: Vec<String>, selected: usize }`
   - Store presets in `~/.config/lo-phi/presets/`

2. **Validation Preview:** Show real-time stats as thresholds change
   - Add field: `WizardData::preview_stats: Option<PreviewStats>`
   - Compute after each threshold step using fast sampling

3. **History Navigation:** Undo/redo through step history
   - Add field: `WizardState::history: Vec<(usize, WizardData)>`
   - Ctrl+Z/Ctrl+Y for history navigation

### Extensibility Points

- `WizardStep` is extensible via new enum variants
- `WizardData` fields can be added without breaking existing steps
- Validation logic is centralized in `validate_step()` function
- Rendering logic is isolated in step-specific `render_*()` functions

---

## Summary

This data model provides:

✓ **Type-safe state management** via Rust enums and structs
✓ **Clear state transitions** with explicit step sequencing
✓ **Comprehensive validation** at field and step levels
✓ **Backward compatibility** with existing Config and Cli structs
✓ **Flexible navigation** supporting forward/backward/quit flows
✓ **Extensible design** for future wizard enhancements

The wizard mode wraps the existing configuration system with a guided UX, ensuring all required parameters are collected before pipeline execution.
