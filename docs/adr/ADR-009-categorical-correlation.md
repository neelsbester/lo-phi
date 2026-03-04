# ADR-009: Categorical Feature Association Measures

**Status:** Accepted
**Date:** 2026-03-03

---

## Context

Lo-phi's correlation analysis stage previously operated exclusively on numeric features, computing Pearson |r| for each numeric-numeric pair and dropping one feature from pairs exceeding the threshold. Categorical features were silently excluded from this stage entirely.

Credit scoring datasets routinely include categorical features such as `employment_type`, `loan_purpose`, `region`, and `industry_sector`. These features can be strongly associated with each other while appearing unrelated to numeric features — for example, `city` and `postal_code` are nearly one-to-one mappings, and including both wastes degrees of freedom in downstream models. Excluding categoricals from redundancy detection left a known gap in the reduction pipeline.

Three distinct feature-pair types require different association measures because Pearson correlation is undefined for non-numeric data:

- **Numeric–Numeric:** Pearson |r| — already implemented, unchanged
- **Categorical–Categorical:** Requires a symmetric, normalized chi-squared-based statistic
- **Categorical–Numeric:** Requires a measure of how much a numeric variable's variance is explained by the categorical grouping

A further gap existed in the tie-breaking logic for which feature to drop from a correlated pair. The previous approach used frequency (category count) as a secondary criterion after the correlation magnitude. For credit scoring, the feature with lower Information Value (IV) is the more principled drop candidate: it contributes less predictive signal, making the IV-first ordering align with the modeler-challenger paradigm.

**Key Factors:**
- All three measures must produce values in [0, 1] to be directly comparable against the single correlation threshold
- High-cardinality categoricals (>100 unique categories) produce unreliable chi-squared statistics due to sparse contingency table cells
- IV results from the preceding pipeline stage are available in memory and can be reused without recomputation
- Missing ratio per feature is already computed in the missing analysis stage and is available for tie-breaking

## Decision

**Chosen Solution:** Three association measures unified under a single [0, 1] scale, selected by feature-pair type

1. **Pearson |r|** for numeric–numeric pairs (unchanged from prior implementation)
2. **Bias-corrected Cramér's V** (Bergsma & Wicherts, 2013) for categorical–categorical pairs
3. **Correlation ratio η (Eta)** for categorical–numeric pairs

All three measures are computed per-pair and compared against the same user-configured correlation threshold. When a pair exceeds the threshold, the drop candidate is chosen by IV-first priority ordering: lowest IV → highest frequency (cardinality) → highest missing ratio → alphabetical. The measure type is recorded in the output report for regulatory documentation.

**Bias-corrected Cramér's V formula:**

```
φ²c = max(0, χ²/N - (k-1)(r-1)/(N-1))
r̃  = r - (r-1)²/(N-1)
k̃  = k - (k-1)²/(N-1)
V̂  = sqrt(φ²c / min(r̃-1, k̃-1))
```

where r = rows, k = columns of the contingency table, N = total observations.

**Correlation ratio η formula:**

```
η² = SS_between / SS_total
η  = sqrt(SS_between / SS_total)
```

where SS_between is the variance explained by the categorical grouping and SS_total is the total variance of the numeric feature.

**High-cardinality guard:** Any pair where either categorical feature has >100 unique categories is skipped, and a warning is logged. This prevents unreliable results from sparse contingency tables.

## Alternatives Considered

### Alternative 1: Theil's U (Uncertainty Coefficient) for Cat–Cat

**Description:** Theil's U measures the conditional entropy reduction: `U(X|Y) = (H(X) - H(X|Y)) / H(X)`. Two variants exist (U(X|Y) and U(Y|X)), typically averaged or reported as a matrix.

**Pros:**
- Information-theoretic foundation — directly measures dependency in bits
- Well-suited to asymmetric relationships (e.g., knowing `postal_code` almost fully determines `city` but not vice versa)
- No chi-squared approximation required for small samples

**Cons:**
- Asymmetric: `U(X|Y) ≠ U(Y|X)`, so a single threshold comparison requires choosing an aggregation (max, min, mean), introducing an arbitrary choice
- Harder to explain to non-technical stakeholders in regulatory settings; Cramér's V maps more directly to "correlation" intuition
- Implementation requires computing full joint and marginal entropy tables, more complex than contingency-table chi-squared

**Rejection Reason:** The asymmetry requires an undocumented aggregation choice that could produce unexpected drop decisions. Cramér's V provides a symmetric, well-understood measure that maps cleanly to the existing threshold paradigm.

---

### Alternative 2: Point-Biserial Correlation for Cat–Num

**Description:** Treat a binary categorical feature as a 0/1 indicator and compute standard Pearson r with the numeric feature. For a two-category feature, this is mathematically equivalent to `rpb = (M1 - M0) / SD_total * sqrt(n1*n0/n²)`.

**Pros:**
- Direct extension of existing Pearson infrastructure — nearly zero new code
- Mathematically identical to Pearson r for binary indicators
- Familiar to statisticians

**Cons:**
- Only valid for binary categorical features (two categories). Extension to multi-category features requires one dummy variable per category, then taking the maximum r across dummies, which is ad hoc
- Sensitive to category imbalance — rare categories produce unreliable coefficients
- Does not generalize naturally: a 5-category feature requires 4 dummies, making threshold comparison inconsistent with the binary case

**Rejection Reason:** The binary-only restriction is too narrow for real credit scoring data, where nominal features routinely have 3–20 categories (employment type, product type, industry code). The correlation ratio η handles arbitrary category counts naturally.

---

### Alternative 3: Separate Thresholds per Measure Type

**Description:** Expose three distinct CLI parameters — `--correlation-threshold` (Pearson), `--cramers-v-threshold` (cat-cat), `--eta-threshold` (cat-num) — each independently configurable.

**Pros:**
- Maximum user control over sensitivity per measure type
- Avoids the implicit assumption that Pearson 0.7 is "equivalent" to Cramér's V 0.7

**Cons:**
- Three thresholds triple the configuration surface; most users will have no basis to differentiate them and will set all to the same value
- UX complexity: the TUI would require three separate threshold input steps
- Documentation burden: explaining the semantic difference between the three scales would be significant
- The [0, 1] ranges of all three measures are comparable in practice (0.7 on any scale represents a strong association)

**Rejection Reason:** The complexity cost exceeds the practical benefit for the target user (credit risk modeler). A single threshold with a visible measure-type label in the output report provides the auditability benefit without the configuration burden.

## Consequences

### Positive Outcomes

- **Complete categorical coverage:** Categorical features now participate fully in the correlation analysis stage, closing the known gap in the reduction pipeline. Redundant pairs like `(city, postal_code)` or `(loan_purpose_group, loan_purpose_detail)` are detected and resolved.
- **IV-first drop logic:** The modeler-challenger paradigm is honoured — the feature with lowest predictive power (IV) is dropped first, rather than the feature with more categories or that comes later alphabetically. This produces more defensible model inputs.
- **Regulatory transparency:** The `measure` column in the reduction report CSV records which association measure was used for each correlated pair (`pearson`, `cramers_v`, `eta`), satisfying SR 11-7 documentation requirements for model development.
- **Consistent threshold semantics:** A single threshold value applies across all three measures, simplifying both the TUI configuration and user documentation.

### Negative Outcomes / Trade-offs

- **Compute overhead:** Building contingency tables for categorical–categorical pairs and computing grouped variance for categorical–numeric pairs adds time proportional to the number of categorical features and their unique value counts. For datasets with many high-cardinality categoricals, this may be noticeable.
- **High-cardinality exclusion:** Pairs where any categorical feature exceeds 100 unique categories are skipped with a warning. This is a hard guard, not a graceful degradation. Features like free-text fields or ID-like categoricals are silently excluded from cat-cat and cat-num analysis.
- **Equivalence assumption:** Treating Pearson 0.7 as equivalent to Cramér's V 0.7 is a pragmatic approximation, not a mathematical equivalence. Users should be aware that the scales are calibrated similarly but not identically.

### Neutral / Future Considerations

- **WoE-space correlation for categoricals:** IV analysis already maps categorical values to WoE scores. A future enhancement could compute Pearson correlation on WoE-encoded categoricals instead of using Cramér's V, measuring association in the "predictive space" rather than the raw category space. This would require converting categorical WoE bins back to per-row numeric values.
- **Cardinality threshold configurability:** The current 100-unique-category guard is hardcoded. A future `--max-category-cardinality` CLI parameter could allow users to raise or lower this limit for their specific dataset characteristics.

## Implementation Notes

**Key Files:**
- `src/pipeline/correlation.rs` - `AssociationMeasure` enum, `FeatureMetadata` struct, `FeatureToDrop` struct; `compute_cramers_v()`, `compute_eta()`, and the unified `run_correlation_analysis()` that dispatches by feature-pair type
- `src/report/reduction_report.rs` - `measure` and `drop_reason` columns added to the reduction report CSV output
- `tests/test_correlation.rs` - Tests for cat-cat, cat-num, and mixed-type pairs; high-cardinality guard; IV-first drop ordering

**New Types:**
```rust
AssociationMeasure { Pearson, CramersV, Eta }

FeatureMetadata {
    iv: Option<f64>,
    missing_ratio: Option<f64>,
}

FeatureToDrop {
    feature: String,
    reason: String,
}
```

**Drop Priority Logic:**
```
1. Lowest IV (if available from Gini/IV stage)
2. Highest unique category count (most complex feature)
3. Highest missing ratio
4. Alphabetical (deterministic tie-break)
```

## References

- Cramér, H. (1946). *Mathematical Methods of Statistics*. Princeton University Press.
- Bergsma, W. & Wicherts, J. (2013). "A bias-correction for Cramér's V and Tschuprow's T." *Journal of the Korean Statistical Society*, 42(3), 323-328.
- Correlation ratio (η): https://en.wikipedia.org/wiki/Correlation_ratio
- Theil's U (uncertainty coefficient): https://en.wikipedia.org/wiki/Uncertainty_coefficient
