# Contract: ADR Template

**Version:** 1.0.0
**Date:** 2026-02-01
**Status:** Draft

---

## Overview

This contract defines the standard template and structure for Architectural Decision Records (ADRs) in the Lo-phi project. All ADRs must follow this template to ensure consistency, completeness, and maintainability.

## Template Structure

```markdown
# ADR-NNN: [Title]

**Status:** [Draft | Accepted | Superseded | Deprecated]
**Date:** [YYYY-MM-DD]
**Supersedes:** [ADR-XXX] (optional, if applicable)
**Superseded by:** [ADR-YYY] (optional, if applicable)

---

## Context

[2-4 paragraphs describing the problem space, technical constraints, and why a decision was needed. Include relevant background on the system state at the time of the decision.]

**Key Factors:**
- [Factor 1: e.g., Performance requirements]
- [Factor 2: e.g., Maintainability concerns]
- [Factor 3: e.g., Licensing constraints]
- [Factor 4: e.g., Ecosystem maturity]

## Decision

[1-2 paragraphs clearly stating what was decided. Be specific and unambiguous.]

**Chosen Solution:** [Name of chosen technology/pattern/approach]

## Alternatives Considered

### Alternative 1: [Name]

**Description:** [1-2 sentences describing the alternative]

**Pros:**
- [Pro 1]
- [Pro 2]

**Cons:**
- [Con 1]
- [Con 2]

**Rejection Reason:** [Why this was not chosen]

---

### Alternative 2: [Name]

**Description:** [1-2 sentences]

**Pros:**
- [Pro 1]
- [Pro 2]

**Cons:**
- [Con 1]
- [Con 2]

**Rejection Reason:** [Why this was not chosen]

---

[Repeat for each alternative - minimum 2 alternatives required]

## Consequences

### Positive Outcomes ✅

- **[Outcome 1]:** [Description with impact]
- **[Outcome 2]:** [Description with impact]
- **[Outcome 3]:** [Description with impact]

### Negative Outcomes / Trade-offs ❌

- **[Trade-off 1]:** [Description with mitigation strategy if applicable]
- **[Trade-off 2]:** [Description with mitigation strategy if applicable]

### Neutral / Future Considerations 🔄

- **[Consideration 1]:** [Description of ongoing maintenance or future implications]

## Implementation Notes

[Optional section: brief notes on how the decision was implemented, key files affected, or integration points. Keep concise.]

**Key Files:**
- `[file path]` - [Purpose]
- `[file path]` - [Purpose]

**Dependencies:**
- [Dependency 1] (version constraint if applicable)
- [Dependency 2]

## References

[Optional section: links to external resources, academic papers, documentation, or related ADRs]

- [Reference 1]
- [Reference 2]

---

**Reviewed by:** [Name/Role] (optional)
**Approved by:** [Name/Role] (optional)
```

---

## Field Specifications

### ADR Number (NNN)

- **Format:** Zero-padded 3-digit number (e.g., 001, 002, ..., 123)
- **Sequence:** Incrementing, never reused
- **Assignment:** Next available number in sequence

### Title

- **Format:** Concise, descriptive phrase (5-10 words)
- **Style:** Title case, no ending punctuation
- **Content:** Clearly identifies the decision subject
- **Examples:**
  - ✅ "Polars as DataFrame Framework"
  - ✅ "HiGHS Solver for Binning Optimization"
  - ❌ "Using Polars" (too vague)
  - ❌ "Why we chose Polars over DataFusion and pandas" (too long)

### Status

- **Valid Values:**
  - `Draft` - ADR is being written, not yet accepted
  - `Accepted` - ADR represents current system state
  - `Superseded` - Decision has been replaced (reference `Superseded by` field)
  - `Deprecated` - Decision no longer applies but kept for history

- **State Transitions:**
  ```
  Draft → Accepted
  Accepted → Superseded (when newer ADR replaces it)
  Accepted → Deprecated (when feature is removed)
  ```

### Date

- **Format:** ISO 8601 (`YYYY-MM-DD`)
- **Meaning:** Date the decision was made (for Draft, use creation date)
- **Update:** Date does NOT change when status changes (preserve historical record)

### Context

**Purpose:** Explain the problem space and why a decision was needed.

**Required Content:**
- Problem statement (what needed to be solved)
- Technical constraints (performance, compatibility, licensing)
- System state at time of decision (what existed, what was missing)
- Why the status quo was insufficient

**Length:** 150-300 words (2-4 paragraphs)

**Key Factors:**
- List 3-5 key factors that influenced the decision
- Each factor should be measurable or verifiable
- Examples: performance requirements, license compatibility, ecosystem maturity

**Style:**
- Past tense for historical context
- Present tense for ongoing constraints
- Objective, factual tone
- No speculation about future

### Decision

**Purpose:** State clearly what was decided.

**Required Content:**
- Unambiguous statement of the chosen solution
- Brief explanation of how it addresses the context
- No justification here (that's in Alternatives/Consequences)

**Length:** 50-150 words (1-2 paragraphs)

**Style:**
- Declarative statements
- No conditional language ("we might", "we could")
- Specific technology/pattern names (no generic descriptions)

### Alternatives Considered

**Purpose:** Document options that were evaluated but not chosen.

**Minimum Alternatives:** 2 (prefer 3-4 for significant decisions)

**Structure per Alternative:**
1. **Name:** Clear identifier (technology/pattern name)
2. **Description:** 1-2 sentences explaining the approach
3. **Pros:** 2-4 positive aspects
4. **Cons:** 2-4 negative aspects
5. **Rejection Reason:** 1-2 sentences explaining why not chosen

**Style:**
- Balanced: acknowledge strengths even for rejected options
- Specific: avoid vague statements like "not as good"
- Factual: base on measurable criteria where possible

**Invalid Alternatives:**
- ❌ "Do nothing" (unless genuinely considered)
- ❌ "Custom build everything" (unless seriously evaluated)
- ❌ Straw-man options included only to justify chosen solution

### Consequences

**Purpose:** Document outcomes of the decision, both positive and negative.

**Required Sections:**

1. **Positive Outcomes ✅**
   - Benefits realized from the decision
   - How it solved the original problem
   - Unexpected positive side effects
   - Minimum 3 outcomes

2. **Negative Outcomes / Trade-offs ❌**
   - Costs or limitations introduced
   - Ongoing maintenance burden
   - Constraints imposed on future work
   - Minimum 2 outcomes
   - **Include mitigation strategies where applicable**

3. **Neutral / Future Considerations 🔄** (optional)
   - Monitoring requirements
   - Re-evaluation triggers
   - Known unknowns

**Style:**
- Be honest about trade-offs (don't sugarcoat)
- Quantify impact where possible (performance, complexity)
- Use specific examples from codebase

### Implementation Notes

**Purpose:** Provide practical information about where the decision manifests in code.

**Content:**
- Key files affected
- Dependencies added (with version constraints)
- Integration points with other modules
- Brief migration notes if replacing previous approach

**Length:** 50-150 words

**Style:**
- Concise, bullet-point format
- File paths relative to repository root
- Version constraints where applicable

### References

**Purpose:** Link to external resources that informed the decision.

**Content:**
- Official documentation for chosen technology
- Academic papers (if applicable)
- Benchmark results or performance analyses
- Related ADRs
- RFC or proposal documents

**Style:**
- Full URLs (not shortened)
- Include title/description for each link
- Prefer stable, authoritative sources

---

## Quality Checklist

Before marking an ADR as "Accepted":

- [ ] **Completeness:**
  - [ ] All required sections present
  - [ ] Minimum 2 alternatives documented
  - [ ] Both positive and negative consequences listed
  - [ ] Context explains why decision was needed

- [ ] **Accuracy:**
  - [ ] Decision matches current codebase
  - [ ] File paths reference actual files
  - [ ] Dependencies match `Cargo.toml`
  - [ ] Consequences are verifiable

- [ ] **Clarity:**
  - [ ] Title clearly identifies decision subject
  - [ ] No ambiguous language in Decision section
  - [ ] Rejection reasons are specific and justified
  - [ ] Trade-offs are honestly presented

- [ ] **Style:**
  - [ ] Markdown renders correctly
  - [ ] No typos or grammatical errors
  - [ ] Professional, objective tone
  - [ ] Consistent formatting (headings, bullets)

- [ ] **Traceability:**
  - [ ] Referenced by architecture.md or other documents
  - [ ] Related ADRs cross-referenced (if applicable)
  - [ ] Implementation notes reference actual code locations

---

## Examples

### Good ADR Title
✅ **ADR-005: Welford Algorithm for Correlation Calculation**
- Specific technology/approach
- Clearly identifies purpose
- Concise

### Poor ADR Title
❌ **ADR-005: Better Correlation**
- Too vague
- Doesn't identify approach
- Doesn't explain "better" criteria

---

### Good Context Section

```markdown
## Context

Lo-phi computes Pearson correlation coefficients for all feature pairs to identify
redundant features. With datasets containing 50+ features, this results in 1000+
correlation calculations, each processing potentially millions of data points.

The naive formula for Pearson correlation, `r = cov(X,Y) / (σ_X * σ_Y)`, requires
computing means first, then making a second pass to calculate covariances. This
two-pass approach doubles memory reads. Additionally, when feature values are
large (e.g., monetary amounts in billions), the naive formula suffers from
catastrophic cancellation—subtracting two large numbers to get a small difference
leads to loss of precision.

At the time of this decision, correlation analysis was the slowest pipeline stage,
taking 60% of total runtime on large datasets. We needed a method that was both
fast (single-pass) and numerically stable.

**Key Factors:**
- Performance: Need to minimize data passes for large datasets
- Numerical Stability: Must handle large feature values without precision loss
- Parallelization: Should enable parallel computation across feature pairs
- Simplicity: Implementation must be maintainable
```

**Why this is good:**
- Quantifies the problem (1000+ calculations, millions of data points)
- Explains technical issues (two-pass, catastrophic cancellation)
- Provides concrete impact (60% of runtime)
- Lists clear evaluation criteria

---

### Good Consequences Section

```markdown
## Consequences

### Positive Outcomes ✅

- **Single-pass efficiency:** Correlation now computed in one pass over data,
  reducing memory reads by 50% and improving performance by ~35% on datasets
  with >1M rows.

- **Numerical stability:** Welford's online updates avoid catastrophic
  cancellation. Testing with synthetic data containing values in range [1e9, 1e12]
  shows correlation accuracy within 1e-14 of ground truth (vs 1e-6 with naive
  formula).

- **Parallelization:** Accumulators for different feature pairs can be computed
  independently and combined via Rayon, scaling linearly with CPU cores.

### Negative Outcomes / Trade-offs ❌

- **Implementation complexity:** Welford algorithm requires tracking 6 accumulators
  per feature pair (count, mean_x, mean_y, M2_x, M2_y, coproduct) vs 2 for naive
  formula (sum_x, sum_y). This increased code complexity from ~20 to ~80 lines.
  **Mitigation:** Added inline documentation and unit tests for accumulator updates.

- **Slight overhead for small datasets:** For datasets with <10,000 rows, the
  numerical stability benefits are negligible, and the extra accumulator updates
  add ~5% overhead. However, correlation is already fast for small datasets
  (<100ms), so this is acceptable.

### Neutral / Future Considerations 🔄

- **Monitoring:** If users report correlation accuracy issues, check for edge
  cases not covered by current tests (e.g., features with extreme outliers).

- **Potential optimization:** For very large datasets (>100M rows), consider
  chunked processing to improve cache locality, though parallelization already
  provides good performance.
```

**Why this is good:**
- Quantifies performance improvements (50% reduction, 35% faster)
- Provides measurable accuracy metrics (1e-14 vs 1e-6)
- Honestly acknowledges trade-offs (complexity, small dataset overhead)
- Includes mitigation strategies
- Defines monitoring/re-evaluation triggers

---

## Anti-Patterns to Avoid

### 1. Justifying the Decision in Hindsight
❌ **Wrong:**
```markdown
We chose Polars because it's the best DataFrame library for Rust.
```

✅ **Right:**
```markdown
We chose Polars over DataFusion and pandas because it provided the best
combination of performance (2x faster than DataFusion on our benchmarks),
pure-Rust implementation (no Python dependency), and native CSV/Parquet
support (DataFusion required custom readers).
```

### 2. Vague Trade-offs
❌ **Wrong:**
```markdown
- Some complexity added
- Might be harder to maintain
```

✅ **Right:**
```markdown
- Implementation complexity increased from ~100 lines to ~300 lines due to
  HiGHS FFI bindings and constraint generation logic. **Mitigation:** Added
  comprehensive unit tests and inline documentation for constraint building.
```

### 3. Strawman Alternatives
❌ **Wrong:**
```markdown
### Alternative 1: Write our own DataFrame library from scratch
**Rejection Reason:** Too much effort, not feasible.
```

✅ **Right:**
```markdown
### Alternative 1: DataFusion
**Description:** Apache Arrow-based query engine with DataFrame API.
**Pros:** Mature, active development, SQL support.
**Cons:** Heavier runtime, query engine features we don't need, more complex API.
**Rejection Reason:** Benchmarks showed 2x slower performance on our workload,
and SQL support was unnecessary for our use case.
```

### 4. No Negative Consequences
❌ **Wrong:**
```markdown
### Negative Outcomes ❌
- None! This decision was perfect.
```

✅ **Right:**
```markdown
### Negative Outcomes ❌
- Build time increased by 30 seconds due to HiGHS C++ compilation.
- Cross-compilation more complex (requires C++ toolchain for target platform).
- Solver licensing (MIT) must be tracked for compliance.
```

**Every decision has trade-offs. Document them honestly.**

---

## File Naming Convention

**Pattern:** `ADR-NNN-kebab-case-title.md`

**Examples:**
- `ADR-001-polars-framework.md`
- `ADR-002-highs-solver.md`
- `ADR-005-welford-correlation.md`

**Rules:**
- Lowercase only
- Hyphens separate words
- No special characters except hyphen
- No file extension abbreviations (use `.md`, not `.markdown`)

---

## Maintenance and Lifecycle

### Creating a New ADR

1. **Assign number:** Use next sequential number (check `docs/adr/` directory)
2. **Create file:** `docs/adr/ADR-NNN-title.md`
3. **Set status:** Start with `Draft`
4. **Fill template:** Complete all required sections
5. **Review:** Technical review against codebase
6. **Acceptance:** Change status to `Accepted`, commit to repository
7. **Update references:** Add link from `architecture.md` if applicable

### Superseding an ADR

When a decision is replaced:

1. **Create new ADR** with new decision
2. **Update old ADR:**
   - Change status to `Superseded`
   - Add `Superseded by: ADR-NNN` field
3. **Update new ADR:**
   - Add `Supersedes: ADR-XXX` field
4. **Commit both changes** in same commit

### Deprecating an ADR

When a feature is removed but decision is historically relevant:

1. **Update ADR:**
   - Change status to `Deprecated`
   - Add note in Context or Consequences explaining removal
2. **Keep file:** Do not delete (preserves history)

---

## Success Criteria

An ADR is considered complete and high-quality when:

1. A developer unfamiliar with the codebase can understand **why** the decision was made
2. The decision can be **verified** against the current codebase (files, dependencies, behavior)
3. Trade-offs are **honestly documented** (not just positives)
4. Alternatives are **fairly evaluated** (not strawman arguments)
5. Future maintainers can **re-evaluate** the decision with full context
6. The ADR **complements** (not duplicates) the architecture documentation

---

## Related Documents

- `spec.md` - FR-6 defines ADR requirements
- `data-model.md` - Entity definition for ADR Collection
- `doc-structure.md` - Individual ADR specifications (ADR-001 through ADR-008)
- `architecture.md` - References ADRs for architectural rationale
