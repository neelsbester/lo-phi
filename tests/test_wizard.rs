//! Unit tests for the TUI wizard state machine
//!
//! These tests verify the wizard's pure logic components:
//! - Step sequencing and navigation
//! - Data validation
//! - State transitions
//! - Default values
//!
//! DO NOT test TUI rendering or terminal operations here - those require
//! integration tests with mocked terminal interfaces.

use lophi::cli::wizard::{
    validate_parquet_extension, validate_schema_inference, validate_threshold,
};
use lophi::cli::wizard::{WizardData, WizardState, WizardStep, WizardTask};

// ============================================================================
// T041: Test reduction path step sequencing
// ============================================================================

#[test]
fn test_reduction_path_step_sequencing() {
    let mut wizard = WizardState::new();

    // Set task to Reduction and build steps
    wizard.data.task = Some(WizardTask::Reduction);
    wizard.data.available_columns = vec!["col1".to_string(), "col2".to_string()];
    wizard.build_steps();

    // Verify we have exactly 8 steps for reduction without optional settings
    // (FileSelection removed - file selector opens inline from TaskSelection)
    assert_eq!(
        wizard.steps.len(),
        8,
        "Reduction path without optional settings should have 8 steps"
    );

    // Verify step order
    assert!(
        matches!(wizard.steps[0], WizardStep::TaskSelection),
        "Step 0 should be TaskSelection"
    );
    assert!(
        matches!(wizard.steps[1], WizardStep::TargetSelection { .. }),
        "Step 1 should be TargetSelection"
    );
    assert!(
        matches!(wizard.steps[2], WizardStep::TargetMapping { .. }),
        "Step 2 should be TargetMapping"
    );
    assert!(
        matches!(wizard.steps[3], WizardStep::MissingThreshold { .. }),
        "Step 3 should be MissingThreshold"
    );
    assert!(
        matches!(wizard.steps[4], WizardStep::GiniThreshold { .. }),
        "Step 4 should be GiniThreshold"
    );
    assert!(
        matches!(wizard.steps[5], WizardStep::CorrelationThreshold { .. }),
        "Step 5 should be CorrelationThreshold"
    );
    assert!(
        matches!(wizard.steps[6], WizardStep::OptionalSettingsPrompt),
        "Step 6 should be OptionalSettingsPrompt"
    );
    assert!(
        matches!(wizard.steps[7], WizardStep::Summary),
        "Step 7 should be Summary"
    );
}

// ============================================================================
// T042: Test conversion path step sequencing
// ============================================================================

#[test]
fn test_conversion_path_step_sequencing() {
    let mut wizard = WizardState::new();

    // Set task to Conversion and build steps
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();

    // Verify we have exactly 4 steps for conversion
    // (FileSelection removed - file selector opens inline from TaskSelection)
    assert_eq!(wizard.steps.len(), 4, "Conversion path should have 4 steps");

    // Verify step order
    assert!(
        matches!(wizard.steps[0], WizardStep::TaskSelection),
        "Step 0 should be TaskSelection"
    );
    assert!(
        matches!(wizard.steps[1], WizardStep::OutputPath { .. }),
        "Step 1 should be OutputPath"
    );
    assert!(
        matches!(wizard.steps[2], WizardStep::ConversionMode { .. }),
        "Step 2 should be ConversionMode"
    );
    assert!(
        matches!(wizard.steps[3], WizardStep::Summary),
        "Step 3 should be Summary"
    );
}

// ============================================================================
// T043: Tests for step navigation
// ============================================================================

#[test]
fn test_next_step_advances() {
    let mut wizard = WizardState::new();
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();

    // Start at index 0
    assert_eq!(wizard.current_index, 0);

    // Advance
    wizard.next_step().unwrap();
    assert_eq!(wizard.current_index, 1, "Should advance to step 1");

    wizard.next_step().unwrap();
    assert_eq!(wizard.current_index, 2, "Should advance to step 2");
}

#[test]
fn test_prev_step_goes_back() {
    let mut wizard = WizardState::new();
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();

    // Advance to step 2
    wizard.next_step().unwrap();
    wizard.next_step().unwrap();
    assert_eq!(wizard.current_index, 2);

    // Go back
    wizard.prev_step().unwrap();
    assert_eq!(wizard.current_index, 1, "Should go back to step 1");

    wizard.prev_step().unwrap();
    assert_eq!(wizard.current_index, 0, "Should go back to step 0");
}

#[test]
fn test_prev_step_boundary_at_first() {
    let mut wizard = WizardState::new();
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();

    // Already at index 0
    assert_eq!(wizard.current_index, 0);

    // Try to go back - should stay at 0
    wizard.prev_step().unwrap();
    assert_eq!(
        wizard.current_index, 0,
        "Should not go below index 0 when at first step"
    );
}

#[test]
fn test_next_step_boundary_at_last() {
    let mut wizard = WizardState::new();
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();

    // Advance to last step (index 3 for 4 steps)
    wizard.current_index = 3;
    assert!(wizard.is_last_step());

    // Try to advance - should stay at last step
    wizard.next_step().unwrap();
    assert_eq!(
        wizard.current_index, 3,
        "Should stay at last step when trying to advance past end"
    );
}

#[test]
fn test_is_last_step() {
    let mut wizard = WizardState::new();
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();

    // Not at last step
    assert!(
        !wizard.is_last_step(),
        "Should not be at last step initially"
    );

    // Advance to last step
    wizard.current_index = wizard.steps.len() - 1;
    assert!(wizard.is_last_step(), "Should be at last step");
}

// ============================================================================
// T044: Tests for threshold validation
// ============================================================================

#[test]
fn test_threshold_validation_valid_values() {
    assert!(
        validate_threshold(0.0).is_ok(),
        "0.0 should be valid threshold"
    );
    assert!(
        validate_threshold(0.5).is_ok(),
        "0.5 should be valid threshold"
    );
    assert!(
        validate_threshold(1.0).is_ok(),
        "1.0 should be valid threshold"
    );
    assert!(
        validate_threshold(0.30).is_ok(),
        "0.30 should be valid threshold"
    );
    assert!(
        validate_threshold(0.99).is_ok(),
        "0.99 should be valid threshold"
    );
}

#[test]
fn test_threshold_validation_invalid_values() {
    assert!(
        validate_threshold(-0.1).is_err(),
        "-0.1 should be invalid threshold"
    );
    assert!(
        validate_threshold(1.1).is_err(),
        "1.1 should be invalid threshold"
    );
    assert!(
        validate_threshold(-1.0).is_err(),
        "-1.0 should be invalid threshold"
    );
    assert!(
        validate_threshold(2.0).is_err(),
        "2.0 should be invalid threshold"
    );
}

// ============================================================================
// T045: Tests for schema inference validation
// ============================================================================

#[test]
fn test_schema_inference_validation_valid_values() {
    assert!(
        validate_schema_inference(0).is_ok(),
        "0 (full scan) should be valid"
    );
    assert!(
        validate_schema_inference(100).is_ok(),
        "100 should be valid"
    );
    assert!(
        validate_schema_inference(10000).is_ok(),
        "10000 should be valid"
    );
    assert!(
        validate_schema_inference(500).is_ok(),
        "500 should be valid"
    );
}

#[test]
fn test_schema_inference_validation_invalid_values() {
    assert!(
        validate_schema_inference(50).is_err(),
        "50 should be invalid (< 100)"
    );
    assert!(
        validate_schema_inference(1).is_err(),
        "1 should be invalid (< 100)"
    );
    assert!(
        validate_schema_inference(99).is_err(),
        "99 should be invalid (< 100)"
    );
}

// ============================================================================
// T046: Tests for output path validation
// ============================================================================

#[test]
fn test_parquet_extension_validation_valid() {
    assert!(
        validate_parquet_extension("output.parquet").is_ok(),
        "output.parquet should be valid"
    );
    assert!(
        validate_parquet_extension("/path/to/file.parquet").is_ok(),
        "Full path with .parquet should be valid"
    );
    assert!(
        validate_parquet_extension("FILE.PARQUET").is_ok(),
        "Uppercase .PARQUET should be valid"
    );
    assert!(
        validate_parquet_extension("file.Parquet").is_ok(),
        "Mixed case .Parquet should be valid"
    );
}

#[test]
fn test_parquet_extension_validation_invalid() {
    assert!(
        validate_parquet_extension("output.csv").is_err(),
        "output.csv should be invalid"
    );
    assert!(
        validate_parquet_extension("output.txt").is_err(),
        "output.txt should be invalid"
    );
    assert!(
        validate_parquet_extension("output").is_err(),
        "No extension should be invalid"
    );
    assert!(
        validate_parquet_extension("parquet").is_err(),
        "Just 'parquet' should be invalid"
    );
}

// ============================================================================
// T047: Test WizardData and WizardState initialization
// ============================================================================

#[test]
fn test_wizard_data_default_values() {
    let data = WizardData::default();

    assert!(data.task.is_none(), "Default task should be None");
    assert!(data.input.is_none(), "Default input should be None");
    assert!(data.target.is_none(), "Default target should be None");
    assert!(
        data.target_mapping.is_none(),
        "Default target_mapping should be None"
    );
    assert_eq!(
        data.missing_threshold, 0.30,
        "Default missing threshold should be 0.30"
    );
    assert_eq!(
        data.gini_threshold, 0.05,
        "Default gini threshold should be 0.05"
    );
    assert_eq!(
        data.correlation_threshold, 0.40,
        "Default correlation threshold should be 0.40"
    );
    assert!(data.use_solver, "Default use_solver should be true");
    assert_eq!(
        data.monotonicity, "none",
        "Default monotonicity should be 'none'"
    );
    assert!(
        data.weight_column.is_none(),
        "Default weight_column should be None"
    );
    assert!(
        data.columns_to_drop.is_empty(),
        "Default columns_to_drop should be empty"
    );
    assert_eq!(
        data.infer_schema_length, 10000,
        "Default infer_schema_length should be 10000"
    );
    assert!(
        data.conversion_output.is_none(),
        "Default conversion_output should be None"
    );
    assert!(
        data.conversion_fast,
        "Default conversion_fast should be true"
    );
    assert!(
        data.available_columns.is_empty(),
        "Default available_columns should be empty"
    );
    assert!(
        data.target_unique_values.is_empty(),
        "Default target_unique_values should be empty"
    );
}

#[test]
fn test_wizard_state_new_starts_at_index_zero() {
    let wizard = WizardState::new();

    assert_eq!(
        wizard.current_index, 0,
        "New wizard should start at index 0"
    );
    assert_eq!(
        wizard.steps.len(),
        1,
        "New wizard should have 1 initial step"
    );
    assert!(
        matches!(wizard.steps[0], WizardStep::TaskSelection),
        "Initial step should be TaskSelection"
    );
    assert!(
        !wizard.show_quit_confirm,
        "Quit confirmation should not be shown initially"
    );
    assert_eq!(
        wizard.task_selected_index, 0,
        "Task selected index should start at 0"
    );
    assert!(
        !wizard.optional_yes,
        "Optional settings should be No by default"
    );
}

#[test]
fn test_wizard_state_build_steps_produces_correct_count() {
    let mut wizard = WizardState::new();

    // Test Reduction without optional settings
    wizard.data.task = Some(WizardTask::Reduction);
    wizard.data.available_columns = vec!["col1".to_string(), "col2".to_string()];
    wizard.optional_yes = false;
    wizard.build_steps();
    assert_eq!(
        wizard.steps.len(),
        8,
        "Reduction without optional settings should have 8 steps"
    );

    // Test Reduction with optional settings
    wizard.optional_yes = true;
    wizard.build_steps();
    assert_eq!(
        wizard.steps.len(),
        13,
        "Reduction with optional settings should have 13 steps (8 base + 5 optional)"
    );

    // Test Conversion
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();
    assert_eq!(wizard.steps.len(), 4, "Conversion should have 4 steps");
}

// ============================================================================
// T048: Test incomplete wizard data handling
// ============================================================================

#[test]
fn test_wizard_state_without_task_has_one_step() {
    let mut wizard = WizardState::new();

    // Don't set a task, just call build_steps
    wizard.build_steps();

    assert_eq!(
        wizard.steps.len(),
        1,
        "Wizard without task should only have TaskSelection step"
    );
    assert!(
        matches!(wizard.steps[0], WizardStep::TaskSelection),
        "Single step should be TaskSelection"
    );
    assert_eq!(
        wizard.current_index, 0,
        "Current index should be reset to 0"
    );
}

// ============================================================================
// T049: Test step titles
// ============================================================================

#[test]
fn test_step_titles_are_correct() {
    // Test each step variant has the correct title
    let steps = [
        (WizardStep::TaskSelection, "Task Selection"),
        (
            WizardStep::TargetSelection {
                search: String::new(),
                filtered: vec![],
                selected: 0,
            },
            "Target Column",
        ),
        (
            WizardStep::TargetMapping {
                unique_values: vec![],
                event_selected: None,
                non_event_selected: None,
                focus: lophi::cli::wizard::TargetMappingFocus::Event,
            },
            "Target Mapping",
        ),
        (
            WizardStep::MissingThreshold {
                input: String::new(),
                error: None,
            },
            "Missing Threshold",
        ),
        (
            WizardStep::GiniThreshold {
                input: String::new(),
                error: None,
            },
            "Gini Threshold",
        ),
        (
            WizardStep::CorrelationThreshold {
                input: String::new(),
                error: None,
            },
            "Correlation Threshold",
        ),
        (WizardStep::OptionalSettingsPrompt, "Optional Settings"),
        (
            WizardStep::SolverToggle { selected: true },
            "Solver Configuration",
        ),
        (
            WizardStep::MonotonicitySelection { selected: 0 },
            "Monotonicity Constraint",
        ),
        (
            WizardStep::WeightColumn {
                search: String::new(),
                filtered: vec![],
                selected: 0,
            },
            "Weight Column",
        ),
        (
            WizardStep::DropColumns {
                search: String::new(),
                filtered: vec![],
                selected: 0,
                checked: vec![],
            },
            "Drop Columns",
        ),
        (
            WizardStep::SchemaInference {
                input: String::new(),
                error: None,
            },
            "Schema Inference",
        ),
        (WizardStep::Summary, "Summary"),
        (
            WizardStep::OutputPath {
                input: String::new(),
                error: None,
            },
            "Output Path",
        ),
        (
            WizardStep::ConversionMode { selected: 0 },
            "Conversion Mode",
        ),
    ];

    for (step, expected_title) in &steps {
        assert_eq!(
            step.title(),
            *expected_title,
            "Step title mismatch for {:?}",
            step
        );
    }
}

// ============================================================================
// T061: Test optional steps insertion
// ============================================================================

#[test]
fn test_optional_steps_insertion_when_yes() {
    let mut wizard = WizardState::new();

    wizard.data.task = Some(WizardTask::Reduction);
    wizard.data.available_columns = vec!["col1".to_string(), "col2".to_string()];
    wizard.optional_yes = true;
    wizard.build_steps();

    // Should have 13 steps total (8 base + 5 optional)
    assert_eq!(wizard.steps.len(), 13);

    // Verify optional steps are inserted before Summary
    assert!(matches!(wizard.steps[7], WizardStep::SolverToggle { .. }));
    assert!(matches!(
        wizard.steps[8],
        WizardStep::MonotonicitySelection { .. }
    ));
    assert!(matches!(wizard.steps[9], WizardStep::WeightColumn { .. }));
    assert!(matches!(wizard.steps[10], WizardStep::DropColumns { .. }));
    assert!(matches!(
        wizard.steps[11],
        WizardStep::SchemaInference { .. }
    ));
    assert!(matches!(wizard.steps[12], WizardStep::Summary));
}

// ============================================================================
// T062: Edge case - empty column list handling
// ============================================================================

#[test]
fn test_reduction_with_empty_columns() {
    let mut wizard = WizardState::new();

    wizard.data.task = Some(WizardTask::Reduction);
    wizard.data.available_columns = vec![]; // Empty columns
    wizard.build_steps();

    // Should still build steps properly
    assert_eq!(wizard.steps.len(), 8);

    // Check that TargetSelection has empty filtered list
    if let WizardStep::TargetSelection { filtered, .. } = &wizard.steps[1] {
        assert!(filtered.is_empty(), "Filtered list should be empty");
    } else {
        panic!("Step 1 should be TargetSelection");
    }
}

// ============================================================================
// T063: Edge case - threshold validation with special values
// ============================================================================

#[test]
fn test_threshold_validation_with_negative_zero() {
    // -0.0 should be valid (equivalent to 0.0)
    assert!(validate_threshold(-0.0).is_ok(), "-0.0 should be valid");
}

#[test]
fn test_threshold_validation_with_nan() {
    // NaN should be invalid
    assert!(
        validate_threshold(f64::NAN).is_err(),
        "NaN should be invalid"
    );
}

#[test]
fn test_threshold_validation_with_infinity() {
    // Infinity should be invalid
    assert!(
        validate_threshold(f64::INFINITY).is_err(),
        "Infinity should be invalid"
    );
    assert!(
        validate_threshold(f64::NEG_INFINITY).is_err(),
        "Negative infinity should be invalid"
    );
}

// ============================================================================
// T064: Edge case - step navigation with optional steps
// ============================================================================

#[test]
fn test_navigation_after_optional_steps_inserted() {
    let mut wizard = WizardState::new();

    wizard.data.task = Some(WizardTask::Reduction);
    wizard.data.available_columns = vec!["col1".to_string()];
    wizard.optional_yes = false;
    wizard.build_steps();

    // Navigate to OptionalSettingsPrompt (step 6)
    wizard.current_index = 6;
    assert!(matches!(
        wizard.steps[6],
        WizardStep::OptionalSettingsPrompt
    ));

    // Simulate user saying "Yes" to optional settings
    wizard.optional_yes = true;
    wizard.build_steps();

    // Should now have 13 steps
    assert_eq!(wizard.steps.len(), 13);

    // Current index should still be valid
    assert!(wizard.current_index < wizard.steps.len());

    // Next step should be the first optional step
    wizard.next_step().unwrap();
    assert!(matches!(
        wizard.steps[wizard.current_index],
        WizardStep::SolverToggle { .. }
    ));
}

// ============================================================================
// Additional edge case tests
// ============================================================================

#[test]
fn test_current_step_returns_correct_step() {
    let mut wizard = WizardState::new();
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();

    // Get current step (should be TaskSelection)
    let step = wizard.current_step();
    assert!(step.is_some());
    assert!(matches!(step.unwrap(), WizardStep::TaskSelection));

    // Advance and check again (FileSelection removed, next is OutputPath)
    wizard.next_step().unwrap();
    let step = wizard.current_step();
    assert!(matches!(step.unwrap(), WizardStep::OutputPath { .. }));
}

#[test]
fn test_current_step_returns_none_for_invalid_index() {
    let mut wizard = WizardState::new();
    wizard.data.task = Some(WizardTask::Conversion);
    wizard.data.input = Some(std::path::PathBuf::from("test.csv"));
    wizard.build_steps();

    // Force invalid index
    wizard.current_index = 999;
    let step = wizard.current_step();
    assert!(step.is_none(), "Should return None for out-of-bounds index");
}
