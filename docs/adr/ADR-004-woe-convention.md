# ADR-004: WoE Sign Convention (ln(Bad/Good))

**Status:** Accepted
**Date:** 2026-02-01

---

## Context

Weight of Evidence (WoE) is a transformation used in credit risk modeling to quantify the predictive power of feature bins. Two mathematical conventions exist in the literature, differing only in the sign of the logarithm. The choice affects interpretation of WoE values and downstream model coefficients.

The "ln(Bad/Good)" convention produces positive WoE for high-risk bins (more events than expected), while "ln(Good/Bad)" produces positive WoE for low-risk bins. Both conventions are mathematically equivalent and produce identical Information Value (IV) statistics, but the sign choice affects scorecard interpretability and alignment with industry practices. Lo-phi must pick one convention and apply it consistently across all features.

**Key Factors:**
- Mathematical equivalence - both conventions produce identical IV and Gini metrics
- Industry alignment - credit risk practitioners expect specific sign conventions
- Scorecard interpretation - positive coefficients should indicate increased risk
- Code clarity - formula should match comments and documentation

## Decision

**Chosen Solution:** `WoE = ln(Bad/Good)` convention where Bad = events (target=1), Good = non-events (target=0)

This convention produces positive WoE values for bins with higher-than-average risk (event rate exceeds population event rate), making interpretation intuitive: higher WoE bins are riskier.

## Alternatives Considered

### Alternative 1: ln(Good/Bad) Convention

**Description:** WoE = ln(%Good / %Bad), producing positive values for low-risk bins and negative values for high-risk bins.

**Pros:**
- Used in some academic papers and older textbooks
- Aligns with "good customer" perspective (positive values = good)
- Logistic regression coefficients are positive for protective features

**Cons:**
- Counter-intuitive for risk modeling - high risk gets negative scores
- Inverted interpretation requires mental flipping ("more negative = worse")
- Less common in modern credit risk industry tools (SAS, Python scorecardpy)
- Confuses practitioners migrating from standard tools

**Rejection Reason:** Industry practice overwhelmingly favors ln(Bad/Good). Using the opposite convention would create confusion when comparing Lo-phi results with other tools and published scorecards.

---

### Alternative 2: Unsigned WoE (Absolute Values)

**Description:** Use |WoE| to eliminate sign ambiguity, focusing only on magnitude of separation.

**Pros:**
- Eliminates sign convention debate entirely
- Simplifies interpretation to "how different is this bin from average"
- IV calculation remains unchanged

**Cons:**
- Loses directional information - cannot distinguish high-risk from low-risk bins
- Scorecard development requires signed WoE for coefficient calculation
- Incompatible with standard logistic regression weight calculation
- Breaks established WoE methodology from credit risk literature

**Rejection Reason:** Directional information is essential for scorecard development. Unsigned WoE is unusable for downstream modeling tasks, defeating the purpose of WoE transformation.

## Consequences

### Positive Outcomes

- **Industry Alignment:** Results directly comparable with SAS, scorecardpy, and other standard tools without sign inversion.
- **Intuitive Interpretation:** Positive WoE clearly indicates "higher risk than average," matching practitioner expectations and simplifying report communication.
- **Scorecard Compatibility:** Positive regression coefficients naturally increase predicted risk, matching conventional credit scoring methodology.

### Negative Outcomes / Trade-offs

- **Migration Friction:** Users familiar with ln(Good/Bad) from older systems must adjust mental models. Mitigated by clear documentation in WoE bin output (column headers and JSON metadata).

### Neutral / Future Considerations

- **Standardization:** Industry is converging on ln(Bad/Good) convention as modern tools adopt this approach. Lo-phi's choice future-proofs against evolving standards.

## Implementation Notes

**Key Files:**
- `src/pipeline/iv.rs` - Line 1466: `let woe = (dist_events / dist_non_events).ln();`
- `src/pipeline/iv.rs` - Line 1465: Comment documents convention: "WoE = ln(%bad / %good) - higher WoE means higher risk"
- `src/report/gini_export.rs` - JSON metadata includes `"woe_convention": "ln(events/non_events)"`

**Dependencies:**
- None (pure Rust standard library `f64::ln()`)

## References

- Naeem Siddiqi (2006), "Credit Risk Scorecards" - Establishes ln(Bad/Good) as standard
- Anderson (2007), "The Credit Scoring Toolkit" - Chapter 4 discusses both conventions, recommends ln(Bad/Good)
- Basel Committee on Banking Supervision - Internal ratings-based approach documentation
