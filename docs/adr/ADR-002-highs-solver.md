# ADR-002: HiGHS Mixed Integer Programming Solver

**Status:** Accepted
**Date:** 2026-02-01

---

## Context

Lo-phi's optimal binning feature requires finding globally optimal bin boundaries that maximize Information Value (IV) subject to constraints like bin count limits, minimum samples per bin, and optional monotonicity (e.g., WoE must increase monotonically with feature values). This is a Mixed Integer Programming (MIP) problem with binary decision variables indicating bin merges.

The naive greedy approach (merge adjacent bins iteratively) produces locally optimal solutions but often misses global optima, especially under monotonicity constraints. A proper MIP solver can explore the solution space systematically using branch-and-bound, guaranteeing optimality within specified gap tolerances. The solver must integrate cleanly with Rust, provide reasonable performance (solving within 30s timeout), and avoid restrictive licenses.

**Key Factors:**
- MIP capability required (binary variables + continuous objective function)
- Integration with Rust codebase without external C/C++ library dependencies
- Ability to enforce custom constraints (monotonicity, bin count, minimum samples)
- Acceptable performance (solve 20-bin problems in <30 seconds)
- Permissive open-source license compatible with MIT/Apache-2.0

## Decision

**Chosen Solution:** HiGHS solver via good_lp crate (v1.8, features = ["highs"])

HiGHS is a high-performance open-source MIP solver with pure-Rust bindings through good_lp. It provides excellent performance on medium-scale MIP problems (up to 1000 variables) and supports custom constraints through linear inequalities. The good_lp crate abstracts the solver interface, allowing potential backend swaps if needed.

## Alternatives Considered

### Alternative 1: CBC (COIN-OR Branch and Cut)

**Description:** Mature open-source MIP solver from the COIN-OR project, widely used in academia and industry with extensive battle-testing.

**Pros:**
- Extremely mature (20+ years of development) with proven reliability
- Comprehensive MIP capabilities including cutting plane generation
- Available through good_lp crate as alternative backend

**Cons:**
- Requires external C++ library installation (coinor-cbc package)
- Significantly slower than HiGHS on medium-scale problems (2-3x in benchmarks)
- Deployment complexity requiring users to install system libraries
- Rust bindings are less maintained than HiGHS integration

**Rejection Reason:** External C++ dependency breaks the project's goal of a standalone Rust binary. Performance disadvantage and deployment friction outweigh maturity benefits.

---

### Alternative 2: GLPK (GNU Linear Programming Kit)

**Description:** Well-established MIP solver under GPL license, commonly used in academic and open-source projects.

**Pros:**
- Widely available on Linux systems through package managers
- Comprehensive documentation and textbooks reference GLPK
- Good performance on small-to-medium problems

**Cons:**
- GPL license creates licensing conflicts with permissive Rust ecosystem
- Derivative works must also be GPL-licensed (copyleft requirement)
- Users incorporating Lo-phi as a library would inherit GPL obligations
- Slower than HiGHS on modern hardware

**Rejection Reason:** GPL license incompatible with Lo-phi's MIT license. Copyleft requirements would restrict downstream usage and integration into commercial projects.

---

### Alternative 3: Custom Greedy Solver

**Description:** Implement a custom heuristic solver using greedy bin merging with local search refinements and simulated annealing.

**Pros:**
- No external dependencies beyond standard Rust crates
- Full control over algorithm tuning and constraint handling
- Lightweight implementation (<500 lines of code)

**Cons:**
- Cannot guarantee global optimality - only local optima
- Monotonicity constraints difficult to enforce without backtracking
- Requires extensive tuning for different dataset characteristics
- Development and testing effort equivalent to several weeks of work
- Would still need MIP solver for validation/benchmarking

**Rejection Reason:** Engineering effort to build, tune, and validate a custom solver is unjustified when high-quality open-source MIP solvers exist. Inability to guarantee optimality undermines the "optimal binning" feature's value proposition.

## Consequences

### Positive Outcomes

- **Provable Optimality:** Solver guarantees optimal solutions within 1% MIP gap (configurable), eliminating guesswork from bin boundary selection.
- **Constraint Flexibility:** Monotonicity constraints (ascending, descending, peak, valley) can be enforced directly in the MIP model through linear inequalities.
- **Pure Rust Deployment:** HiGHS bundled statically with the binary, eliminating runtime dependencies and simplifying distribution across platforms.

### Negative Outcomes / Trade-offs

- **Solve Time:** Optimal binning adds 5-30 seconds per feature depending on problem complexity. Mitigated by making solver optional (users can fall back to fast greedy CART binning with `--no-solver` flag).
- **Memory Overhead:** MIP model construction requires ~10MB per feature for large prebinning (100+ bins), acceptable for modern systems but noteworthy for embedded environments.

### Neutral / Future Considerations

- **Solver Backend Portability:** good_lp abstraction allows swapping HiGHS for alternative solvers if performance/license requirements change, though HiGHS currently meets all needs.

## Implementation Notes

**Key Files:**
- `Cargo.toml` - `good_lp = { version = "1.8", default-features = false, features = ["highs"] }`
- `src/pipeline/solver/mod.rs` - Solver configuration and result types
- `src/pipeline/solver/model.rs` - MIP model construction (binary variables, IV objective, constraints)
- `src/pipeline/solver/monotonicity.rs` - Monotonicity constraint enforcement

**Dependencies:**
- `good_lp = { version = "1.8", default-features = false, features = ["highs"] }`

## References

- HiGHS Documentation: https://highs.dev/
- good_lp Crate Documentation: https://docs.rs/good_lp/
- MIP Formulation for Optimal Binning: Mironchyk & Tchistiakov (2017), "Monotone optimal binning algorithm for credit risk modeling"
