# Architectural Decision Records (ADRs)

This directory contains Architectural Decision Records documenting key design decisions in the Lo-phi project.

## What are ADRs?

ADRs capture important architectural decisions along with their context and consequences. Each record explains:
- Why a decision was needed (Context)
- What was decided (Decision)
- What alternatives were considered (Alternatives)
- What outcomes resulted (Consequences)

## ADR Index

| ADR | Title | Category | Status |
|-----|-------|----------|--------|
| [ADR-001](ADR-001-polars-framework.md) | Polars DataFrame Framework | Infrastructure | Accepted |
| [ADR-002](ADR-002-highs-solver.md) | HiGHS MIP Solver | Algorithm | Accepted |
| [ADR-003](ADR-003-cart-default-binning.md) | CART Default Binning | Algorithm | Accepted |
| [ADR-004](ADR-004-woe-convention.md) | WoE Convention ln(Bad/Good) | Algorithm | Accepted |
| [ADR-005](ADR-005-welford-correlation.md) | Welford Correlation Algorithm | Algorithm | Accepted |
| [ADR-006](ADR-006-sequential-pipeline.md) | Sequential Pipeline Stages | Architecture | Accepted |
| [ADR-007](ADR-007-dual-file-format.md) | Dual CSV/Parquet Support | Infrastructure | Accepted |
| [ADR-008](ADR-008-ratatui-tui.md) | Ratatui Terminal UI | User Interface | Accepted |

## Reading Guide

### For New Contributors

Start with these ADRs to understand core architectural choices:
1. **ADR-001** (Polars) - Data structure foundation
2. **ADR-006** (Sequential Pipeline) - Overall architecture
3. **ADR-008** (Ratatui TUI) - User interaction model

### For Algorithm Implementers

If working on binning, WoE, or correlation algorithms:
1. **ADR-003** (CART Binning) - Default binning strategy
2. **ADR-004** (WoE Convention) - Critical for scorecard compatibility
3. **ADR-005** (Welford Correlation) - Numerical stability approach
4. **ADR-002** (HiGHS Solver) - Optimal binning with constraints

### For Infrastructure Changes

If modifying I/O, dependencies, or deployment:
1. **ADR-001** (Polars) - DataFrame framework choice
2. **ADR-007** (CSV/Parquet) - File format support
3. **ADR-002** (HiGHS) - Solver dependency rationale

## ADR Template

All ADRs follow a consistent template:

```markdown
# ADR-NNN: [Title]

**Status:** Accepted | Deprecated | Superseded
**Date:** YYYY-MM-DD

## Context
[Problem description and constraints]

## Decision
[What was decided]

## Alternatives Considered
[At least 2 realistic alternatives with pros/cons]

## Consequences
[Positive outcomes, negative outcomes, future considerations]

## Implementation Notes
[Key files and dependencies]

## References
[External links and papers]
```

## Contributing

When making significant architectural changes:
1. Create a new ADR with the next sequential number
2. Follow the template structure
3. Include at least 2 realistic alternatives (no strawman arguments)
4. Document both positive and negative consequences
5. Link to relevant code files
6. Update this README index

## Status Definitions

- **Accepted**: Currently in use, represents active architecture
- **Deprecated**: No longer recommended, kept for historical context
- **Superseded**: Replaced by a newer ADR (link to successor)

---

**Last Updated:** 2026-02-01
