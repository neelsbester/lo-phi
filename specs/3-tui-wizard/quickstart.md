# TUI Wizard Mode - Developer Quickstart Guide

This guide helps contributors quickly get started with implementing and testing the TUI Wizard Mode feature for lo-phi.

## What is TUI Wizard Mode?

A step-by-step interactive terminal interface for configuring lo-phi's feature reduction pipeline. Guides users through file selection, target column selection, threshold configuration, and advanced options before running the reduction pipeline.

## Prerequisites

- Rust toolchain installed
- Clone the repository and checkout branch `3-tui-wizard`
- Familiarity with terminal UIs (ratatui) is helpful but not required

## Essential Reading (In Order)

1. **`specs/3-tui-wizard/spec.md`** - Feature specification and requirements (read first)
2. **`specs/3-tui-wizard/research.md`** - Technical decisions and ratatui patterns
3. **`specs/3-tui-wizard/data-model.md`** - State machine and data structures
4. **`specs/3-tui-wizard/plan.md`** - Implementation roadmap and tasks
5. **`src/cli/config_menu.rs`** - Existing TUI patterns to follow (focus on lines 174-285 and 520-652)

## Quick Build & Test Commands

```bash
# Development workflow
cargo build                              # Debug build
cargo test --all-features                # Run all tests
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo fmt                                # Format code
make check                               # Full CI check (format + lint + test)

# Testing specific components
cargo test --lib --all-features          # Unit tests only
cargo test --test '*' --all-features     # Integration tests only
cargo test --test test_wizard -- --nocapture  # Wizard tests with output

# Release build
cargo build --release
```

## Project Structure

### Files You'll Modify
```
src/cli/
├── mod.rs              # Add `pub mod wizard;` here
└── wizard.rs           # NEW: Main wizard implementation (~600-800 lines)

tests/
└── test_wizard.rs      # NEW: Wizard tests (~200-300 lines)
```

### Files to Reference (Don't Modify Unless Necessary)
```
src/cli/
├── args.rs             # CLI argument parsing (211 lines)
├── config_menu.rs      # Existing TUI patterns (2648 lines)
└── convert.rs          # CSV to Parquet conversion

src/main.rs             # Pipeline orchestrator (772 lines)
tests/test_cli.rs       # CLI parsing tests (261 lines)
tests/common/mod.rs     # Shared test fixtures
```

## Key Dependencies

- **Ratatui** (`0.29+`) - TUI rendering framework
- **Crossterm** - Terminal I/O and event handling
- **Polars** - DataFrame operations (for loading column names)
- **Clap** - CLI argument parsing (not used in wizard, but config struct is shared)
- **Anyhow** - Error handling

## Core Types and Patterns

### State Machine (Indexed Steps)
```rust
/// Individual wizard step with embedded UI state
pub enum WizardStep {
    TaskSelection,
    FileSelection,
    TargetSelection { search: String, columns: Vec<String>, filtered: Vec<usize>, selected: usize },
    MissingThreshold { input: String, error: Option<String> },
    // ... more variants (see data-model.md for full list)
    Summary,
}

/// Main wizard state
pub struct WizardState {
    pub steps: Vec<WizardStep>,     // Ordered step sequence
    pub current_index: usize,        // Current position
    pub data: WizardData,            // Accumulated configuration
    pub show_quit_confirm: bool,     // Quit dialog state
}
```

### Config Output (Shared with Existing TUI)
```rust
pub enum ConfigResult {
    Proceed(Box<Config>),    // Run reduction pipeline
    Convert(Box<Config>),    // Run CSV-to-Parquet conversion
    Quit,                    // User cancelled
}

pub struct Config {
    pub input: PathBuf,
    pub output: PathBuf,
    pub target: Option<String>,
    pub missing_threshold: f64,
    pub gini_threshold: f64,
    pub correlation_threshold: f64,
    pub use_solver: bool,
    pub solver_trend: Option<String>,
    pub weight_column: Option<String>,
    pub drop_columns: Vec<String>,
    // ... more fields
}
```

### Search/Filter Pattern (from config_menu.rs)
```rust
// State fields
search_query: String,
all_items: Vec<String>,      // Full list
filtered_items: Vec<String>, // Filtered by search
selected_index: usize,       // Index in filtered_items

// Update function
fn update_filtered(&mut self) {
    self.filtered_items = self.all_items
        .iter()
        .filter(|item| item.to_lowercase().contains(&self.search_query.to_lowercase()))
        .cloned()
        .collect();
    self.selected_index = self.selected_index.min(self.filtered_items.len().saturating_sub(1));
}
```

### Terminal Setup/Teardown Pattern
```rust
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

// Setup
enable_raw_mode()?;
let mut stdout = std::io::stdout();
execute!(stdout, EnterAlternateScreen)?;
let backend = CrosstermBackend::new(stdout);
let mut terminal = Terminal::new(backend)?;

// Teardown (in Drop or explicit cleanup)
disable_raw_mode()?;
execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
terminal.show_cursor()?;
```

## Implementation Steps

### Phase 1: Foundation (Start Here)
1. Create `src/cli/wizard.rs`
2. Add `pub mod wizard;` to `src/cli/mod.rs`
3. Define `WizardState` enum with all variants
4. Implement basic terminal setup/teardown
5. Create main event loop skeleton
6. Test: Run wizard, verify it starts and quits cleanly

### Phase 2: File Selection
1. Implement `FileSelection` state with search/filter
2. Render file list with Ratatui `List` widget
3. Handle keyboard navigation (Up/Down, Enter, Esc)
4. Parse schema inference from filename
5. Test: Navigate files, select one, verify state transition

### Phase 3: Target Selection
1. Load column names from selected file (use Polars `LazyFrame::schema()`)
2. Implement search/filter for columns
3. Render column list with data types
4. Handle selection and "No Target" option
5. Test: Search columns, select target, verify state transition

### Phase 4: Configuration Steps
1. Implement `ThresholdConfig` state (Missing → Gini → Correlation chained editor)
2. Implement `SolverOptions` state (toggle + dropdown for trend)
3. Implement `DataOptions` state (weight column + drop columns multi-select)
4. Test: Edit each configuration, verify values persist

### Phase 5: Summary & Navigation
1. Implement `Summary` state showing all configured values
2. Implement `Help` state overlay (F1 toggles, Esc returns)
3. Add progress indicator (Step N of M)
4. Test: Navigate backward, verify state restoration

### Phase 6: Integration
1. Build final `Config` struct from wizard state
2. Return `ConfigResult::Proceed(config)`
3. Wire up to `main.rs` (add `--manual` flag for dashboard mode; wizard is the default)
4. Test: Complete wizard, verify pipeline runs with correct config

### Phase 7: Testing & Polish
1. Create `tests/test_wizard.rs`
2. Add unit tests for state transitions
3. Add integration tests for config building
4. Run `make check` and fix all issues
5. Update `CLAUDE.md` with wizard documentation

## Ratatui Rendering Patterns

### Basic Draw Function
```rust
fn draw(&mut self, frame: &mut Frame) {
    match &self.state {
        WizardState::FileSelection { .. } => self.draw_file_selection(frame),
        WizardState::TargetSelection { .. } => self.draw_target_selection(frame),
        // ... more states
    }
}

fn draw_file_selection(&self, frame: &mut Frame) {
    use ratatui::{
        layout::{Constraint, Layout},
        style::{Color, Style},
        widgets::{Block, Borders, List, ListItem},
    };

    let area = frame.size();
    // Render widgets to area
}
```

### Event Handling Pattern
```rust
fn handle_event(&mut self, event: Event) -> anyhow::Result<Option<ConfigResult>> {
    if let Event::Key(key) = event {
        match &mut self.state {
            WizardState::FileSelection { .. } => {
                match key.code {
                    KeyCode::Enter => { /* transition */ },
                    KeyCode::Esc => return Ok(Some(ConfigResult::Quit)),
                    KeyCode::Up => { /* navigate */ },
                    // ... more keys
                    _ => {}
                }
            },
            // ... more states
        }
    }
    Ok(None) // Continue running
}
```

## Common Pitfalls

1. **Forgetting terminal cleanup** - Always disable raw mode and leave alternate screen, even on error
2. **Index out of bounds** - Use `.saturating_sub(1)` and `.min()` for list navigation
3. **Empty filtered lists** - Handle case when search returns no results
4. **State transitions** - Ensure all fields are properly transferred between states
5. **F1 help overlay** - Remember to restore previous state when exiting help
6. **Progress indicator** - Keep step numbers consistent with actual flow

## Testing Strategy

### Unit Tests
- State transitions (FileSelection → TargetSelection → etc.)
- Search/filter logic
- Config building from wizard state
- Edge cases (empty lists, invalid thresholds)

### Integration Tests
- Full wizard flow end-to-end
- Backward navigation
- Help overlay
- Config output matches expected format

### Manual Testing
```bash
# Run wizard (default mode, no flag needed)
cargo run --release

# Test with pre-specified input file
cargo run --release -- --input tests/data/sample.parquet

# Test manual/dashboard mode
cargo run --release -- --manual --input tests/data/sample.csv

# Test CLI-only mode
cargo run --release -- --no-confirm --input tests/data/sample.csv --target y
```

## Coding Conventions

- **No emojis** - Use text indicators like `[X]`, `[ ]`, `>`, `*`
- **Error handling** - Use `anyhow::Result` throughout
- **Formatting** - Run `cargo fmt` before every commit
- **Linting** - Fix all `cargo clippy` warnings
- **Comments** - Document state transitions and complex logic
- **Tests** - Add unit tests for new edge cases

## Reference Examples in Existing Code

Study these sections in `src/cli/config_menu.rs`:

- **File selector pattern**: Lines 1885-2016 (`draw_file_selector()`)
- **Search input handling**: Lines 1625-1650 (in `handle_column_selection_event()`)
- **List navigation**: Lines 1615-1625 (Up/Down key handling)
- **State management**: Lines 520-652 (`run_menu_loop()` main loop)
- **Multi-select**: Lines 2018-2145 (`draw_drop_column_selector()`)
- **Help overlay**: Lines 2147-2206 (`draw_help_screen()`)

## Getting Help

- **Spec questions**: Reference `specs/3-tui-wizard/spec.md`
- **Ratatui patterns**: Check `specs/3-tui-wizard/research.md`
- **State machine**: See `specs/3-tui-wizard/data-model.md`
- **Implementation order**: Follow `specs/3-tui-wizard/plan.md`
- **Code patterns**: Study `src/cli/config_menu.rs`

## Pre-Commit Checklist

- [ ] `cargo fmt` - Code is formatted
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` - No lint warnings
- [ ] `cargo test --all-features` - All tests pass
- [ ] `cargo build --release` - Release build succeeds
- [ ] Manual test: Complete wizard flow end-to-end
- [ ] Update `CLAUDE.md` if adding new conventions

## Next Steps After Implementation

1. Update `CLAUDE.md` with wizard mode documentation
2. Add wizard mode to README.md
3. Create demo recording with VHS (see `demo.tape` for example)
4. Submit PR with clear description of changes
5. Address review feedback

---

**Ready to start?** Read `specs/3-tui-wizard/spec.md` first, then dive into `src/cli/config_menu.rs` to understand the existing TUI patterns. Start with Phase 1 (Foundation) and work your way through each phase, testing thoroughly before moving on.
