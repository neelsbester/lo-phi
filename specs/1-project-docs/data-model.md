# Data Model: Comprehensive Project Reference Documentation

**Version:** 1.0.0
**Date:** 2026-02-01
**Status:** Draft

---

> **Relationship to doc-structure.md**: This data model defines entity purposes, relationships, and validation criteria at a conceptual level. The `contracts/doc-structure.md` contract defines the detailed required sections, word counts, and FR/NFR traceability per document. When details conflict, `doc-structure.md` is authoritative for section-level specifications.

## Overview

This data model defines all documentation entities for the Lo-phi project reference documentation suite. Each entity represents a distinct document or collection of documents with defined purpose, content structure, relationships, and validation criteria.

## Documentation Entities

| Entity | Description | File | Relationships |
|--------|-------------|------|---------------|
| Architecture Document | System-level overview with module diagrams | `docs/architecture.md` | References → Algorithm Guide, Developer Guide, ADRs |
| Algorithm Guide | Statistical methods with formulas | `docs/algorithms.md` | References → Glossary, Architecture; Referenced by → Output Reference |
| User Guide | CLI/TUI configuration reference | `docs/user-guide.md` | References → Glossary; Referenced by → Developer Guide |
| Developer Guide | Setup, conventions, testing | `docs/developer-guide.md` | References → Architecture, User Guide, Algorithm Guide |
| Output Reference | File format specifications | `docs/output-reference.md` | References → Algorithm Guide, Glossary |
| ADR Collection | 8 decision records | `docs/adr/ADR-001.md` through `docs/adr/ADR-008.md` | Referenced by → Architecture |
| Glossary | Domain terms | `docs/glossary.md` | Referenced by → All other documents |
| Worked Example | End-to-end pipeline walkthrough | `docs/worked-example.md` | References → All other documents |

## Entity Details

### 1. Architecture Document

**File:** `docs/architecture.md`

**Purpose:** Provide system-level overview of Lo-phi's architecture, module organization, pipeline flow, and high-level design patterns.

**Audience:** New contributors, auditors, developers extending the system

**Key Sections:**
1. System Overview
   - Purpose and core capabilities
   - High-level architecture diagram (text-based)
2. Module Structure
   - CLI module (`src/cli/`)
   - Pipeline module (`src/pipeline/`)
   - Report module (`src/report/`)
   - Utils module (`src/utils/`)
3. Pipeline Flow
   - Sequential processing stages
   - Data transformations
   - State management
4. Key Design Patterns
   - Error handling strategy
   - Progress reporting
   - Parallel processing approach
5. Technology Stack
   - Polars for DataFrame operations
   - Rayon for parallelization
   - Ratatui for TUI
   - HiGHS for optimization

**Source Code References:**
- `src/main.rs` - Main pipeline orchestration
- `src/pipeline/mod.rs` - Pipeline module exports
- `src/cli/mod.rs` - CLI module structure
- `src/report/mod.rs` - Report module structure
- `src/utils/mod.rs` - Utilities

**Cross-Reference Targets:**
- Links to Algorithm Guide for statistical method details
- Links to Developer Guide for contribution guidelines
- Links to ADRs for architectural decision rationale

**Validation Criteria:**
- All modules in `src/` are documented
- Pipeline flow matches `src/main.rs` execution order
- Technology choices align with `Cargo.toml` dependencies
- References to ADRs exist for major design decisions

**Estimated Word Count:** 1200-1500 words

---

### 2. Algorithm Guide

**File:** `docs/algorithms.md`

**Purpose:** Document all statistical methods, formulas, edge-case handling, and mathematical foundations used in Lo-phi.

**Audience:** Data scientists, auditors, statisticians, contributors implementing new algorithms

**Key Sections:**
1. Overview of Feature Selection Methods
   - Missing value analysis
   - Information Value (IV) / Gini coefficient
   - Correlation analysis
2. Weight of Evidence (WoE) Binning
   - Mathematical definition
   - Binning strategies (CART, Quantile)
   - Monotonicity constraints
   - Edge cases (zero events/non-events)
   - Laplace smoothing
3. Information Value (IV) Calculation
   - Formula and interpretation
   - Threshold selection rationale
4. Gini Coefficient
   - Formula derivation
   - Relationship to IV
5. Pearson Correlation
   - Formula
   - Welford algorithm for numerical stability
   - Parallel computation strategy
6. Missing Value Analysis
   - Null ratio calculation
   - Threshold interpretation
7. Solver-Based Binning Optimization
   - Problem formulation (MIP model)
   - Monotonicity constraints
   - Objective function
   - Solver parameters (timeout, gap)
8. Constants and Parameters
   - `DEFAULT_PREBINS = 20`
   - `MIN_BIN_SAMPLES = 5`
   - `SMOOTHING = 0.5`
   - Default thresholds

**Source Code References:**
- `src/pipeline/iv.rs` - WoE/IV implementation (~2600 lines)
- `src/pipeline/correlation.rs` - Correlation with Welford algorithm
- `src/pipeline/missing.rs` - Null ratio calculation
- `src/pipeline/solver/` - Optimization model
  - `src/pipeline/solver/model.rs` - MIP formulation
  - `src/pipeline/solver/monotonicity.rs` - Constraint generation
  - `src/pipeline/solver/precompute.rs` - Trend detection

**Cross-Reference Targets:**
- Links to Glossary for term definitions
- Links to Architecture for module context
- Referenced by Output Reference for field interpretations

**Validation Criteria:**
- All formulas match implementation in source code
- All constants documented with values from source
- Edge cases (e.g., `events == 0`, `non_events == 0`) documented
- Smoothing behavior explicitly described
- Academic references cited where applicable

**Estimated Word Count:** 2000-2500 words (most complex document)

---

### 3. User Guide

**File:** `docs/user-guide.md`

**Purpose:** Comprehensive reference for all CLI arguments, TUI keyboard shortcuts, configuration parameters, and their behavioral effects.

**Audience:** End users, data scientists, analysts configuring the tool

**Key Sections:**
1. Installation and Quick Start
   - Building from source
   - Basic usage example
2. CLI Mode Reference
   - All command-line arguments
   - Default values
   - Valid ranges
   - Examples
3. Interactive TUI Mode
   - Launching interactive mode
   - Three-column layout explanation
   - Keyboard shortcuts
   - Configuration flow
4. Configuration Parameters
   - Thresholds (missing, gini, correlation)
   - Solver options (use solver, trend/monotonicity)
   - Data options (drop columns, weight column, schema inference)
   - Advanced binning parameters (CLI-only)
5. CSV to Parquet Conversion
   - Using the `[F]` shortcut
   - Performance considerations
6. Common Workflows
   - Quick analysis with defaults
   - Custom threshold tuning
   - Using weight columns
   - Solver-based binning

**Source Code References:**
- `src/cli/args.rs` - CLI argument definitions
- `src/cli/config_menu.rs` - TUI implementation (~400 lines)
- `src/cli/convert.rs` - CSV-to-Parquet conversion
- `src/main.rs` - Default configuration values

**Cross-Reference Targets:**
- Links to Glossary for term definitions (WoE, IV, Gini, etc.)
- Referenced by Developer Guide for configuration structure

**Validation Criteria:**
- Every CLI argument from `args.rs` is documented
- All TUI keyboard shortcuts from `config_menu.rs` are documented
- Default values match source code
- Valid ranges match validation logic in source
- TUI layout description matches rendered output

**Estimated Word Count:** 1500-1800 words

---

### 4. Developer Guide

**File:** `docs/developer-guide.md`

**Purpose:** Enable new Rust developers to set up, understand, test, and contribute to the Lo-phi codebase.

**Audience:** New contributors, maintainers, developers extending functionality

**Key Sections:**
1. Development Setup
   - Prerequisites (Rust toolchain)
   - Cloning and building
   - IDE recommendations
2. Project Structure
   - Directory layout
   - Module responsibilities
   - File organization conventions
3. Code Conventions
   - Formatting (`cargo fmt`)
   - Linting (`cargo clippy`)
   - Error handling patterns (`thiserror`, `anyhow`)
   - Naming conventions
4. Testing
   - Running tests (`cargo test`)
   - Test structure (`tests/common/`, integration tests)
   - Fixtures and test helpers
   - Writing new tests
   - Coverage expectations
5. Benchmarking
   - Running benchmarks (`cargo bench`)
   - Benchmark structure
   - Interpreting results
6. Contributing
   - Git workflow
   - Commit message format
   - Pull request process
   - Code review expectations
7. Common Development Tasks
   - Adding a new pipeline stage
   - Adding a new CLI option
   - Adding a new TUI shortcut
   - Updating output formats

**Source Code References:**
- `Cargo.toml` - Dependencies and project metadata
- `Makefile` - Common development commands
- `tests/common/mod.rs` - Test fixtures
- `tests/test_*.rs` - Integration tests
- `benches/binning_benchmark.rs` - Benchmark structure
- `.github/workflows/` - CI configuration (if exists)

**Cross-Reference Targets:**
- Links to Architecture for system overview
- Links to User Guide for configuration structure
- Links to Algorithm Guide for statistical method background

**Validation Criteria:**
- Setup instructions can be followed on a clean system
- All test commands work as documented
- Common tasks have working examples
- Code conventions match actual codebase practices

**Estimated Word Count:** 1800-2200 words

---

### 5. Output Reference

**File:** `docs/output-reference.md`

**Purpose:** Specify the format, schema, and interpretation of all output files generated by Lo-phi.

**Audience:** Data scientists, analysts, downstream consumers of Lo-phi output

**Key Sections:**
1. Output File Overview
   - File naming conventions
   - Output directory structure
2. Reduced Dataset (`{input}_reduced.{csv|parquet}`)
   - Format (CSV or Parquet)
   - Schema (retained columns from input)
   - Row preservation guarantee
3. Reduction Report ZIP Bundle (`{input}_reduction_report.zip`)
   - Bundle contents
   - Extraction instructions
4. Gini Analysis JSON (`{input}_gini_analysis.json`)
   - Top-level schema
   - `IvAnalysis` object structure
   - `WoeBin` fields (numeric features)
   - `CategoricalWoeBin` fields (categorical features)
   - Field data types
   - Example snippet
5. Reduction Report JSON (`{input}_reduction_report.json`)
   - Schema overview
   - Per-feature metadata
   - Correlation pairs
   - Analysis summary
   - Example snippet
6. Reduction Report CSV (`{input}_reduction_report.csv`)
   - Row-per-feature format
   - Column definitions
   - Correlated features pipe-separated format
   - Example row
7. Interpreting Results
   - Understanding IV thresholds
   - Reading WoE values
   - Correlation interpretation
   - Missing value ratios

**Source Code References:**
- `src/report/gini_export.rs` - JSON export logic
- `src/report/reduction_report.rs` - Report generation
- `src/report/summary.rs` - Summary table generation
- `src/pipeline/iv.rs` - `IvAnalysis`, `WoeBin`, `CategoricalWoeBin` types

**Cross-Reference Targets:**
- Links to Algorithm Guide for statistical formula details
- Links to Glossary for term definitions

**Validation Criteria:**
- All output file types are documented
- JSON schemas match Rust struct serialization
- CSV column names match actual output
- Field descriptions are accurate and complete
- Example snippets are valid and representative

**Estimated Word Count:** 1200-1500 words

---

### 6. ADR Collection

**Files:** `docs/adr/ADR-001.md` through `docs/adr/ADR-008.md`

**Purpose:** Capture key architectural and technical decisions with context, alternatives, and consequences.

**Audience:** Developers, auditors, future maintainers

**ADR Topics:**

1. **ADR-001: Polars as DataFrame Framework**
   - Context: Need for high-performance DataFrame operations
   - Decision: Use Polars over alternatives (pandas, DataFusion)
   - Consequences: Pure Rust, lazy evaluation, memory efficiency

2. **ADR-002: HiGHS Solver for Binning Optimization**
   - Context: Need for monotonic binning optimization
   - Decision: Use HiGHS solver over alternatives (CBC, GLPK)
   - Consequences: Performance, licensing, Rust bindings

3. **ADR-003: CART as Default Binning Strategy**
   - Context: Need to choose default binning approach
   - Decision: CART over Quantile for default
   - Consequences: Better class separation, more complex implementation

4. **ADR-004: WoE Sign Convention**
   - Context: Multiple WoE sign conventions exist
   - Decision: Use `ln(Bad/Good)` convention (positive WoE = higher risk)
   - Consequences: Intuitive for credit scoring, industry alignment

5. **ADR-005: Welford Algorithm for Correlation**
   - Context: Numerical stability in correlation calculation
   - Decision: Use Welford's online algorithm
   - Consequences: Single-pass, numerically stable, parallelizable

6. **ADR-006: Sequential Pipeline Architecture**
   - Context: Pipeline stage ordering
   - Decision: Missing → IV → Correlation sequential flow
   - Consequences: Predictable behavior, easier to reason about

7. **ADR-007: Dual File Format Support (CSV & Parquet)**
   - Context: Input/output format flexibility
   - Decision: Support both CSV and Parquet
   - Consequences: Broader compatibility, conversion utility

8. **ADR-008: Ratatui for TUI Framework**
   - Context: Need for interactive configuration
   - Decision: Use Ratatui over alternatives
   - Consequences: Terminal-based, cross-platform, Rust-native

**Source Code References:**
- Each ADR references specific source files related to the decision
- `Cargo.toml` for dependency choices
- Module implementations for algorithmic choices

**Cross-Reference Targets:**
- Referenced by Architecture document
- May link to Algorithm Guide for technical details

**Validation Criteria:**
- Each ADR follows standard template
- Context accurately describes problem
- Alternatives are documented
- Consequences include both positives and negatives
- Decision aligns with current codebase

**Estimated Word Count:** 200-400 words per ADR (1600-3200 total)

---

### 7. Glossary

**File:** `docs/glossary.md`

**Purpose:** Define all domain-specific and technical terms used throughout Lo-phi documentation and codebase.

**Audience:** All users of documentation (universal reference)

**Key Terms to Define:**
- WoE (Weight of Evidence)
- IV (Information Value)
- Gini Coefficient
- Binning / Prebinning
- CART (Classification and Regression Trees)
- Quantile Binning
- Laplace Smoothing
- Monotonicity Constraint
- Null Ratio
- Pearson Correlation
- Welford Algorithm
- Event Rate
- Bad Rate / Good Rate
- Solver (MIP context)
- Gap Tolerance
- Schema Inference
- Population Splitting
- Feature Reduction

**Structure:**
- Alphabetical ordering
- Each entry includes:
  - Term
  - Definition (concise, 1-2 sentences)
  - Context (where it's used in Lo-phi)
  - Related terms (cross-references)
  - Formula (if applicable)

**Source Code References:**
- Terms extracted from module documentation
- Constants and parameters from source

**Cross-Reference Targets:**
- Referenced by all other documents

**Validation Criteria:**
- All terms used in other documentation are defined
- Definitions are accurate and non-circular
- Formulas match algorithm guide
- No orphaned terms (unused in documentation)

**Estimated Word Count:** 800-1200 words

---

### 8. Worked Example

**File:** `docs/worked-example.md`

**Purpose:** Provide an end-to-end walkthrough of the Lo-phi pipeline using a small synthetic dataset with annotated inputs and outputs.

**Audience:** New users, data scientists, auditors validating methodology

**Key Sections:**
1. Example Dataset
   - Synthetic data description (20 rows, 3 numeric features, 2 categorical features, 1 binary target)
   - CSV snippet showing input data
   - Feature descriptions
2. Configuration
   - Selected parameters (thresholds, binning strategy, solver options)
   - Rationale for choices
3. Pipeline Execution
   - Command line used
   - TUI selections (if applicable)
4. Step-by-Step Analysis Results
   - Missing value analysis: which features passed/failed
   - IV/Gini analysis: WoE bins for each feature, IV values
   - Correlation analysis: correlation matrix, dropped features
5. Output Files
   - Reduced dataset: which features remained
   - Gini analysis JSON: annotated snippet
   - Reduction report CSV: annotated row
   - Reduction report JSON: key sections explained
6. Interpretation
   - Why each feature was kept/dropped
   - How to read WoE values
   - Understanding correlation pairs

**Source Code References:**
- None (uses output of running Lo-phi)
- May reference synthetic dataset script if provided

**Cross-Reference Targets:**
- Links to Algorithm Guide for formula context
- Links to Output Reference for field definitions
- Links to User Guide for configuration parameters
- Links to Glossary for term lookups

**Validation Criteria:**
- Example can be reproduced by following instructions
- All output snippets are actual Lo-phi output (not fabricated)
- Annotations correctly explain values
- Covers all major pipeline stages
- Demonstrates at least one feature kept and one dropped per stage

**Estimated Word Count:** 1500-2000 words

---

## State Transitions

Each documentation entity follows a simple lifecycle:

```
Draft → Review → Published
```

### Draft
- Content is being actively written
- May have incomplete sections
- Cross-references may be missing or incorrect

### Review
- All content sections are complete
- Cross-references are in place
- Ready for technical review against source code

### Published
- Content verified against current codebase
- All validation criteria met
- Committed to repository

### State Tracking

Track document state in a `docs/STATUS.md` file:

```markdown
# Documentation Status

| Document | Status | Last Updated | Verified Against Commit |
|----------|--------|--------------|-------------------------|
| architecture.md | Draft | 2026-02-01 | - |
| algorithms.md | Draft | 2026-02-01 | - |
| ... | ... | ... | ... |
```

---

## Relationships and Dependencies

### Dependency Graph

```
glossary.md (no dependencies)
    ↓
architecture.md (references: glossary)
    ↓
algorithm-guide.md (references: glossary, architecture)
    ↓
user-guide.md (references: glossary)
developer-guide.md (references: architecture, user-guide, algorithm-guide)
    ↓
output-reference.md (references: algorithm-guide, glossary)
    ↓
worked-example.md (references: all above)
    ↓
adr/*.md (references: architecture, algorithm-guide as needed)
```

### Implementation Order

Based on dependencies:
1. **glossary.md** (no dependencies, used by all)
2. **architecture.md** (depends on glossary)
3. **Parallel track:**
   - algorithm-guide.md (depends on glossary, architecture)
   - user-guide.md (depends on glossary)
4. **developer-guide.md** (depends on architecture, user-guide, algorithm-guide)
5. **output-reference.md** (depends on algorithm-guide, glossary)
6. **worked-example.md** (depends on all above)
7. **ADRs** (can be written in parallel, minimal dependencies)

---

## Validation Strategy

### Per-Document Validation

Each document must pass validation before moving to "Published" state:

1. **Completeness Check:**
   - All sections from data model are present
   - No placeholder text ("TBD", "TODO")
   - All code references resolve to actual files

2. **Accuracy Check:**
   - Formulas match source code implementation
   - Constants match values in source
   - Configuration defaults match source
   - Output schemas match serialized types

3. **Cross-Reference Check:**
   - All internal links resolve
   - All source code references exist
   - Glossary terms are defined
   - No broken references

4. **Markdown Quality:**
   - Valid Markdown syntax
   - Renders correctly on GitHub
   - Code blocks have language tags
   - LaTeX formulas render correctly

### Cross-Document Validation

After all documents are complete:

1. **Consistency Check:**
   - Terms used consistently across documents
   - No contradictory statements
   - Cross-references are bidirectional where appropriate

2. **Coverage Check:**
   - All source modules are documented
   - All CLI arguments are documented
   - All output fields are documented
   - No significant gaps in functionality

3. **Traceability Check:**
   - All FRs from spec.md are satisfied
   - All success criteria are met
   - All user scenarios can be completed using documentation

---

## Maintenance Strategy

### Keeping Documentation Current

As the codebase evolves:

1. **Source Code Changes:**
   - When adding a new pipeline stage → update architecture.md, developer-guide.md
   - When adding a new CLI option → update user-guide.md
   - When changing a formula → update algorithm-guide.md, verify output-reference.md
   - When changing output format → update output-reference.md

2. **New Features:**
   - Add glossary entries for new terms
   - Create new ADR for significant decisions
   - Update worked example if pipeline behavior changes

3. **Deprecations:**
   - Mark deprecated features in documentation
   - Update examples to use current best practices

### Review Triggers

Documentation review is triggered by:
- Major version releases
- Significant architectural changes
- User-reported documentation bugs
- Failed validation against source code
