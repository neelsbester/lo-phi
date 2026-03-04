//! Dataset sampling utilities for survey and ML workflows.
//!
//! Supports three sampling strategies:
//! - **Random** – simple random sample without replacement
//! - **Stratified** – proportional or user-specified allocation per stratum
//! - **EqualAllocation** – same sample size for every stratum
//!
//! All strategies append a `sampling_weight` column (N_h / n_h) to the
//! sampled DataFrame so that weighted estimators remain unbiased.

use std::path::PathBuf;

use anyhow::{bail, Result};
use polars::prelude::*;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The sampling strategy to apply.
#[derive(Debug, Clone, PartialEq)]
pub enum SamplingMethod {
    /// Simple random sample without replacement.
    Random,
    /// Stratified sample using caller-supplied stratum sizes.
    Stratified,
    /// Equal-allocation stratified sample (same n per stratum).
    EqualAllocation,
}

/// How the desired sample size is expressed.
#[derive(Debug, Clone)]
pub enum SampleSize {
    /// Absolute number of rows.
    Count(usize),
    /// Fraction of the population, must be in `(0.0, 1.0)`.
    Fraction(f64),
}

/// Per-stratum specification used by [`SamplingMethod::Stratified`].
#[derive(Debug, Clone)]
pub struct StratumSpec {
    /// Stratum label (matches values in `strata_column`; `"(null)"` for nulls).
    pub value: String,
    /// Population size for this stratum.
    pub population_count: usize,
    /// Desired sample size for this stratum.
    pub sample_size: usize,
}

/// Full configuration for a sampling run.
#[derive(Debug, Clone)]
pub struct SamplingConfig {
    /// Path to the input dataset file.
    pub input: PathBuf,
    /// Path where the sampled output will be written.
    pub output: PathBuf,
    /// Which sampling strategy to use.
    pub method: SamplingMethod,
    /// Column used to partition rows into strata (required for Stratified /
    /// EqualAllocation, ignored for Random).
    pub strata_column: Option<String>,
    /// Desired sample size (used for Random and EqualAllocation).
    pub sample_size: Option<SampleSize>,
    /// Per-stratum allocation (used for Stratified; built internally for
    /// EqualAllocation).
    pub strata_specs: Vec<StratumSpec>,
    /// Optional RNG seed for reproducibility.
    pub seed: Option<u64>,
    /// Number of rows used when inferring the CSV/Parquet schema.
    pub infer_schema_length: usize,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Convert an [`AnyValue`] to a human-readable label string.
///
/// Unlike `AnyValue::to_string()`, this avoids quoting string values so that
/// `AnyValue::Utf8("A")` returns `"A"` rather than `"\"A\""`.
fn anyvalue_to_label(val: &AnyValue<'_>) -> String {
    match val {
        AnyValue::Null => "(null)".to_string(),
        AnyValue::String(s) => s.to_string(),
        AnyValue::StringOwned(s) => s.to_string(),
        other => other.to_string(),
    }
}

/// Analyse the unique values and counts in a stratum column.
///
/// Returns a vector of `(value, count)` pairs sorted by count **descending**.
/// Null values appear as the literal string `"(null)"`.
///
/// # Errors
/// Returns an error if `column` does not exist in `df`.
pub fn analyze_strata(df: &DataFrame, column: &str) -> Result<Vec<(String, usize)>> {
    let col = df
        .column(column)
        .map_err(|_| anyhow::anyhow!("Strata column '{}' not found in DataFrame", column))?;

    let series = col.as_materialized_series();

    // Build a frequency map in one pass.
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for val in series.iter() {
        let key = anyvalue_to_label(&val);
        *counts.entry(key).or_insert(0) += 1;
    }

    let mut result: Vec<(String, usize)> = counts.into_iter().collect();
    // Sort by count descending, then alphabetically for determinism.
    result.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    Ok(result)
}

/// Execute the sampling operation described by `config` against `df`.
///
/// Returns a new [`DataFrame`] containing only the sampled rows plus an
/// appended `sampling_weight` column (Float64).
///
/// # Errors
/// - `"Column 'sampling_weight' already exists in dataset"` when the column
///   is already present.
/// - `"Sample size must be positive"` when n = 0 or fraction = 0.0.
/// - `"Fraction must be in (0.0, 1.0)"` when fraction >= 1.0.
/// - `"Sample size ({n}) exceeds population size ({N})"` when n > N.
/// - `"strata_column is required for Stratified / EqualAllocation sampling"`
///   when the method requires a strata column but none is provided.
pub fn execute_sampling(df: &DataFrame, config: &SamplingConfig) -> Result<DataFrame> {
    // Guard: sampling_weight column must not already exist.
    if df
        .get_column_names()
        .iter()
        .any(|n| n.as_str() == "sampling_weight")
    {
        bail!("Column 'sampling_weight' already exists in dataset");
    }

    match config.method {
        SamplingMethod::Random => {
            let n = resolve_count(df.height(), config.sample_size.as_ref())?;
            random_sample(df, n, config.seed)
        }
        SamplingMethod::Stratified => {
            let strata_col = config.strata_column.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "strata_column is required for Stratified / EqualAllocation sampling"
                )
            })?;
            stratified_sample(df, strata_col, &config.strata_specs, config.seed)
        }
        SamplingMethod::EqualAllocation => {
            let strata_col = config.strata_column.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "strata_column is required for Stratified / EqualAllocation sampling"
                )
            })?;
            let n = match config.sample_size.as_ref() {
                Some(SampleSize::Count(c)) => {
                    if *c == 0 {
                        bail!("Sample size must be positive");
                    }
                    *c
                }
                Some(SampleSize::Fraction(_)) => {
                    bail!(
                        "Fraction is not applicable to EqualAllocation sampling; use Count instead"
                    );
                }
                None => bail!("sample_size is required for EqualAllocation sampling"),
            };
            equal_allocation_sample(df, strata_col, n, config.seed)
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve an optional [`SampleSize`] into a concrete row count.
fn resolve_count(population: usize, size: Option<&SampleSize>) -> Result<usize> {
    match size {
        Some(SampleSize::Count(n)) => {
            if *n == 0 {
                bail!("Sample size must be positive");
            }
            Ok(*n)
        }
        Some(SampleSize::Fraction(f)) => {
            if *f <= 0.0 {
                bail!("Sample size must be positive");
            }
            if *f >= 1.0 {
                bail!("Fraction must be in (0.0, 1.0)");
            }
            let n = ((*f) * population as f64).round() as usize;
            Ok(n.max(1))
        }
        None => bail!("sample_size is required for Random sampling"),
    }
}

/// Simple random sample of `n` rows without replacement.
///
/// Appends a constant `sampling_weight` column equal to `N / n`.
fn random_sample(df: &DataFrame, n: usize, seed: Option<u64>) -> Result<DataFrame> {
    let cap_n = df.height();
    if n > cap_n {
        bail!("Sample size ({n}) exceeds population size ({cap_n})");
    }

    let mut sampled = df
        .sample_n_literal(n, false, true, seed)
        .map_err(|e| anyhow::anyhow!("Polars sampling error: {}", e))?;

    let weight = cap_n as f64 / n as f64;
    let weight_col = Series::new("sampling_weight".into(), vec![weight; n]);
    sampled
        .with_column(weight_col)
        .map_err(|e| anyhow::anyhow!("Failed to append sampling_weight column: {}", e))?;

    Ok(sampled)
}

/// Stratified sample using caller-supplied [`StratumSpec`] list.
///
/// Rows are partitioned by `strata_column`, each partition is sampled to
/// `spec.sample_size`, and a per-stratum `sampling_weight` (N_h / n_h) is
/// appended before vstacking all groups.
fn stratified_sample(
    df: &DataFrame,
    strata_column: &str,
    specs: &[StratumSpec],
    seed: Option<u64>,
) -> Result<DataFrame> {
    if specs.is_empty() {
        bail!("strata_specs must not be empty for Stratified sampling");
    }

    // Validate the column exists up front.
    let _ = df
        .column(strata_column)
        .map_err(|_| anyhow::anyhow!("Strata column '{}' not found in DataFrame", strata_column))?;

    let strata_series = df.column(strata_column)?.as_materialized_series().clone();
    let mut parts: Vec<DataFrame> = Vec::with_capacity(specs.len());

    for spec in specs {
        let n_h = spec.sample_size;
        let n_pop = spec.population_count;

        // Empty stratum: skip with a warning.
        if n_pop == 0 {
            eprintln!(
                "Warning: stratum '{}' has population_count = 0, skipping",
                spec.value
            );
            continue;
        }

        if n_h == 0 {
            bail!("Sample size must be positive (stratum '{}')", spec.value);
        }
        if n_h > n_pop {
            bail!(
                "Sample size ({n_h}) exceeds population size ({n_pop}) for stratum '{}'",
                spec.value
            );
        }

        // Build a boolean mask for this stratum.
        let mask: BooleanChunked = strata_series
            .iter()
            .map(|v| anyvalue_to_label(&v) == spec.value)
            .collect();

        let stratum_df = df
            .filter(&mask)
            .map_err(|e| anyhow::anyhow!("Filter error for stratum '{}': {}", spec.value, e))?;

        let actual_pop = stratum_df.height();
        if actual_pop == 0 {
            eprintln!(
                "Warning: stratum '{}' is empty in DataFrame, skipping",
                spec.value
            );
            continue;
        }

        let effective_n = n_h.min(actual_pop);
        let mut sampled = stratum_df
            .sample_n_literal(effective_n, false, true, seed)
            .map_err(|e| anyhow::anyhow!("Sampling error for stratum '{}': {}", spec.value, e))?;

        let weight = actual_pop as f64 / effective_n as f64;
        let weight_col = Series::new("sampling_weight".into(), vec![weight; effective_n]);
        sampled
            .with_column(weight_col)
            .map_err(|e| anyhow::anyhow!("Failed to append sampling_weight: {}", e))?;

        parts.push(sampled);
    }

    if parts.is_empty() {
        bail!("All strata were empty; no rows sampled");
    }

    // Stack all stratum samples vertically.
    let mut combined = parts[0].clone();
    for part in &parts[1..] {
        combined = combined
            .vstack(part)
            .map_err(|e| anyhow::anyhow!("vstack error: {}", e))?;
    }

    Ok(combined)
}

/// Equal-allocation stratified sample: `n` rows from every stratum.
///
/// Builds a [`StratumSpec`] for each unique value in `strata_column` and
/// delegates to [`stratified_sample`].
fn equal_allocation_sample(
    df: &DataFrame,
    strata_column: &str,
    n: usize,
    seed: Option<u64>,
) -> Result<DataFrame> {
    let strata = analyze_strata(df, strata_column)?;

    let specs: Vec<StratumSpec> = strata
        .into_iter()
        .map(|(value, population_count)| StratumSpec {
            value,
            population_count,
            sample_size: n,
        })
        .collect();

    stratified_sample(df, strata_column, &specs, seed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_df() -> DataFrame {
        df! {
            "id"    => [1i64, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            "group" => ["A", "A", "A", "A", "B", "B", "B", "C", "C", "C"],
            "value" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
        }
        .unwrap()
    }

    fn make_config(method: SamplingMethod) -> SamplingConfig {
        SamplingConfig {
            input: PathBuf::from("input.csv"),
            output: PathBuf::from("output.csv"),
            method,
            strata_column: None,
            sample_size: None,
            strata_specs: vec![],
            seed: Some(42),
            infer_schema_length: 10_000,
        }
    }

    // --- analyze_strata ---

    #[test]
    fn test_analyze_strata_counts() {
        let df = make_df();
        let strata = analyze_strata(&df, "group").unwrap();
        // A=4, B=3, C=3
        assert_eq!(strata[0].0, "A");
        assert_eq!(strata[0].1, 4);
        // B and C are tied at 3; alphabetical tie-break: B before C
        let labels: Vec<&str> = strata.iter().map(|(l, _)| l.as_str()).collect();
        assert!(labels.contains(&"B"));
        assert!(labels.contains(&"C"));
    }

    #[test]
    fn test_analyze_strata_missing_column() {
        let df = make_df();
        assert!(analyze_strata(&df, "nonexistent").is_err());
    }

    #[test]
    fn test_analyze_strata_null_label() {
        let col: Series = Series::new("g".into(), [Some("X"), None, Some("X"), None]);
        let df = DataFrame::new(vec![col.into()]).unwrap();
        let strata = analyze_strata(&df, "g").unwrap();
        let labels: Vec<&str> = strata.iter().map(|(l, _)| l.as_str()).collect();
        assert!(labels.contains(&"(null)"));
        assert!(labels.contains(&"X"));
    }

    // --- random_sample ---

    #[test]
    fn test_random_sample_row_count() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Count(4));
        let result = execute_sampling(&df, &cfg).unwrap();
        assert_eq!(result.height(), 4);
    }

    #[test]
    fn test_random_sample_weight_column_value() {
        let df = make_df(); // N=10
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Count(5));
        let result = execute_sampling(&df, &cfg).unwrap();
        let weights = result
            .column("sampling_weight")
            .unwrap()
            .f64()
            .unwrap()
            .into_no_null_iter()
            .collect::<Vec<_>>();
        // weight = 10/5 = 2.0
        assert!(weights.iter().all(|&w| (w - 2.0).abs() < 1e-10));
    }

    #[test]
    fn test_random_sample_fraction() {
        let df = make_df(); // N=10
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Fraction(0.3)); // round(3.0) = 3
        let result = execute_sampling(&df, &cfg).unwrap();
        assert_eq!(result.height(), 3);
    }

    #[test]
    fn test_random_sample_exceeds_population() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Count(99));
        let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
        assert!(err.contains("exceeds population size"), "{err}");
    }

    #[test]
    fn test_random_sample_zero_count_errors() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Count(0));
        let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
        assert!(err.contains("Sample size must be positive"), "{err}");
    }

    #[test]
    fn test_random_sample_fraction_zero_errors() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Fraction(0.0));
        let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
        assert!(err.contains("Sample size must be positive"), "{err}");
    }

    #[test]
    fn test_random_sample_fraction_gte_one_errors() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Fraction(1.0));
        let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
        assert!(err.contains("Fraction must be in (0.0, 1.0)"), "{err}");
    }

    #[test]
    fn test_sampling_weight_already_exists_errors() {
        let df = df! {
            "x" => [1i64, 2, 3],
            "sampling_weight" => [1.0f64, 1.0, 1.0],
        }
        .unwrap();
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Count(2));
        let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
        assert!(err.contains("sampling_weight"), "{err}");
    }

    #[test]
    fn test_random_sample_preserves_columns() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Random);
        cfg.sample_size = Some(SampleSize::Count(3));
        let result = execute_sampling(&df, &cfg).unwrap();
        // original 3 columns + sampling_weight
        assert_eq!(result.width(), 4);
        assert!(result.column("sampling_weight").is_ok());
    }

    // --- stratified_sample ---

    #[test]
    fn test_stratified_sample_row_count() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Stratified);
        cfg.strata_column = Some("group".to_string());
        cfg.strata_specs = vec![
            StratumSpec {
                value: "A".to_string(),
                population_count: 4,
                sample_size: 2,
            },
            StratumSpec {
                value: "B".to_string(),
                population_count: 3,
                sample_size: 2,
            },
            StratumSpec {
                value: "C".to_string(),
                population_count: 3,
                sample_size: 1,
            },
        ];
        let result = execute_sampling(&df, &cfg).unwrap();
        assert_eq!(result.height(), 5); // 2+2+1
    }

    #[test]
    fn test_stratified_per_stratum_weight() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Stratified);
        cfg.strata_column = Some("group".to_string());
        cfg.strata_specs = vec![StratumSpec {
            value: "A".to_string(),
            population_count: 4,
            sample_size: 2,
        }];
        let result = execute_sampling(&df, &cfg).unwrap();
        // A has 4 rows, we sample 2 → weight = 4/2 = 2.0
        let weights = result
            .column("sampling_weight")
            .unwrap()
            .f64()
            .unwrap()
            .into_no_null_iter()
            .collect::<Vec<_>>();
        assert!(weights.iter().all(|&w| (w - 2.0).abs() < 1e-10));
    }

    #[test]
    fn test_stratified_sample_exceeds_stratum_errors() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::Stratified);
        cfg.strata_column = Some("group".to_string());
        cfg.strata_specs = vec![StratumSpec {
            value: "B".to_string(),
            population_count: 3,
            sample_size: 99,
        }];
        let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
        assert!(err.contains("exceeds population size"), "{err}");
    }

    // --- equal_allocation_sample ---

    #[test]
    fn test_equal_allocation_row_count() {
        let df = make_df(); // A=4, B=3, C=3
        let mut cfg = make_config(SamplingMethod::EqualAllocation);
        cfg.strata_column = Some("group".to_string());
        cfg.sample_size = Some(SampleSize::Count(2));
        let result = execute_sampling(&df, &cfg).unwrap();
        // 3 strata × 2 rows each = 6
        assert_eq!(result.height(), 6);
    }

    #[test]
    fn test_equal_allocation_no_strata_column_errors() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::EqualAllocation);
        cfg.sample_size = Some(SampleSize::Count(2));
        // strata_column intentionally not set
        let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
        assert!(err.contains("strata_column is required"), "{err}");
    }

    #[test]
    fn test_equal_allocation_fraction_errors() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::EqualAllocation);
        cfg.strata_column = Some("group".to_string());
        cfg.sample_size = Some(SampleSize::Fraction(0.5));
        let err = execute_sampling(&df, &cfg).unwrap_err().to_string();
        assert!(err.contains("Fraction is not applicable"), "{err}");
    }

    #[test]
    fn test_equal_allocation_reproducible_with_seed() {
        let df = make_df();
        let mut cfg = make_config(SamplingMethod::EqualAllocation);
        cfg.strata_column = Some("group".to_string());
        cfg.sample_size = Some(SampleSize::Count(2));
        cfg.seed = Some(99);

        let r1 = execute_sampling(&df, &cfg).unwrap();
        let r2 = execute_sampling(&df, &cfg).unwrap();

        let ids1: Vec<i64> = r1
            .column("id")
            .unwrap()
            .i64()
            .unwrap()
            .into_no_null_iter()
            .collect();
        let ids2: Vec<i64> = r2
            .column("id")
            .unwrap()
            .i64()
            .unwrap()
            .into_no_null_iter()
            .collect();

        assert_eq!(ids1, ids2);
    }
}
