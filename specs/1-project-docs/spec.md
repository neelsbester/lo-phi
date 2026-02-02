# Feature Specification: Comprehensive Project Reference Documentation

**Version:** 1.0.0
**Date:** 2026-02-01
**Status:** Draft

---

## Summary

Generate a comprehensive, detailed reference documentation suite for the Lo-phi project that serves as a single source of truth for all architectural decisions, technical details, algorithmic implementations, configuration options, and operational guidance. This documentation will enable new contributors, auditors, and users to fully understand the system without needing to read the source code, while providing sufficient depth for developers maintaining or extending the codebase.

## Constitution Alignment

| Principle                       | Applicable | Notes                                                                                          |
| ------------------------------- | ---------- | ---------------------------------------------------------------------------------------------- |
| 1. Statistical Correctness      | Yes        | Documentation must accurately describe WoE/IV, Gini, and correlation algorithms with formulas  |
| 2. Performance at Scale         | No         | Documentation generation is a one-time authoring task; no runtime performance impact            |
| 3. Transparent Decision-Making  | Yes        | Documents all architectural decisions with rationale, making design choices auditable           |
| 4. Ergonomic TUI/CLI            | Yes        | User-facing docs improve discoverability of CLI/TUI features and configuration options          |
| 5. Rigorous Testing             | Yes        | Documentation must cover test architecture, fixtures, and how to write new tests                |

## Requirements

### Functional Requirements

1. **[FR-1] Architecture Reference Document**: Produce a document describing the overall system architecture, module responsibilities, data flow through the pipeline, and key design decisions with their rationale.

2. **[FR-2] Algorithm & Statistical Methods Guide**: Produce a document covering all statistical methods used (WoE/IV binning, Gini coefficient, Pearson correlation, Welford algorithm), including mathematical formulas, edge-case handling, smoothing constants, and references to academic sources.

3. **[FR-3] User & Configuration Guide**: Produce a document covering all CLI arguments, TUI keyboard shortcuts, threshold configurations, binning strategy options, solver parameters, and their effects on analysis results.

4. **[FR-4] Developer Onboarding Guide**: Produce a document that enables a new Rust developer to set up the project, understand the codebase structure, run tests, run benchmarks, and contribute code following project conventions.

5. **[FR-5] Output & Report Reference**: Produce a document describing all output file formats (reduced dataset, JSON analysis, CSV summary, ZIP bundle), their schemas, field definitions, and how to interpret the results.

6. **[FR-6] Architectural Decision Records (ADRs)**: Produce a set of decision records capturing key technical choices (e.g., why Polars over DataFusion, why HiGHS solver, why two correlation methods, why CART+Quantile binning strategies) with context, alternatives considered, and consequences.

7. **[FR-7] Cross-Referencing & Navigation**: All documents must cross-reference related sections and link to source code files where relevant, enabling readers to navigate between conceptual explanations and implementation details.

8. **[FR-8] Glossary of Terms**: Produce a glossary defining all domain-specific terms (WoE, IV, Gini, prebins, Laplace smoothing, monotonicity constraints, etc.) used throughout the project.

9. **[FR-9] End-to-End Worked Example**: Include one complete worked example using a small synthetic dataset that walks through the full pipeline (input data, configuration, each analysis stage, and all output files) with annotated results explaining what each value means.

### Non-Functional Requirements

1. **[NFR-1] Accuracy**: All documented algorithms, formulas, and configuration descriptions must exactly match the current implementation, verified by comparing each formula and constant against the corresponding source code location. No aspirational or planned features may be presented as current functionality.

2. **[NFR-2] Maintainability**: Documentation must be structured in a modular way (separate files per concern) so individual sections can be updated independently when the codebase changes.

3. **[NFR-3] Accessibility**: Documentation must be written in standard Markdown, renderable on GitHub, and understandable by someone with intermediate Rust knowledge and basic statistics background.

4. **[NFR-4] Completeness**: Every module's purpose, every configuration option, and every output format must be documented. No significant system behavior should be undocumented. (Function-level API documentation is handled separately by rustdoc.)

## Scope

### In Scope

- Architecture overview with module dependency diagrams (text-based)
- Statistical algorithm documentation with mathematical formulas
- Complete CLI/TUI configuration reference
- Developer setup and contribution guide
- Output file format specifications with field-level descriptions
- Architectural Decision Records for major design choices
- Glossary of domain-specific terms
- Cross-references between documents and to source code
- Test architecture and fixture documentation
- One end-to-end worked example with synthetic dataset and annotated outputs

### Out of Scope

- Auto-generated API documentation (rustdoc handles this separately)
- Video tutorials or screencasts
- Translated versions in languages other than English
- External hosting or documentation site generation (e.g., mdBook, Docusaurus)
- Documentation for planned/future features listed in CLAUDE.md TODO section
- Performance benchmarking methodology documentation (covered by benchmark source code)

## User Scenarios & Testing

### Scenario 1: New Contributor Onboarding

**Actor**: A Rust developer joining the project for the first time.

**Flow**:
1. Developer clones the repository
2. Opens the developer guide and follows setup instructions
3. Reads the architecture overview to understand module structure
4. Reads the algorithm guide to understand the statistical methods
5. Navigates to specific source files using cross-references
6. Runs the test suite following documented commands
7. Successfully submits their first contribution

**Acceptance**: The developer can build, test, and understand the pipeline flow without asking questions that are answered in the documentation.

### Scenario 2: Data Scientist Understanding Results

**Actor**: A data scientist using Lo-phi output files for model building.

**Flow**:
1. Runs Lo-phi on their dataset
2. Opens the output reference documentation
3. Understands each field in the reduction report JSON
4. Interprets WoE/IV values from the Gini analysis JSON
5. Uses the glossary to look up unfamiliar terms
6. Cross-references the algorithm guide for the statistical formulas used

**Acceptance**: The data scientist can correctly interpret all output fields and their statistical meaning without reading source code.

### Scenario 3: Auditor Reviewing Decision Logic

**Actor**: A model risk auditor reviewing feature selection methodology.

**Flow**:
1. Reads the algorithm documentation for IV/Gini calculations
2. Reviews the ADRs for why specific methods were chosen
3. Examines the configuration reference for threshold impacts
4. Traces the pipeline flow from input to output
5. Verifies edge-case handling (missing values, small samples, smoothing)

**Acceptance**: The auditor can produce a complete assessment of the methodology without requiring developer interviews.

### Scenario 4: Existing Developer Adding a New Analysis Stage

**Actor**: A contributor adding a new pipeline stage (e.g., Cramer's V).

**Flow**:
1. Reads the architecture document to understand the pipeline pattern
2. Reviews the developer guide for code conventions and test requirements
3. Studies the algorithm guide to understand how existing analyses are structured
4. Uses ADRs to understand past architectural decisions
5. Implements the new stage following documented patterns

**Acceptance**: The developer can implement a new pipeline stage that follows existing patterns without breaking conventions.

## Key Entities

| Entity                  | Description                                                                 |
| ----------------------- | --------------------------------------------------------------------------- |
| Architecture Document   | System-level overview of modules, data flow, and design patterns            |
| Algorithm Guide         | Statistical methods reference with formulas and edge-case documentation     |
| User Guide              | CLI/TUI configuration reference for end users                              |
| Developer Guide         | Setup, conventions, testing, and contribution instructions                  |
| Output Reference        | File format specifications for all generated outputs                        |
| ADR                     | Individual architectural decision record with context and rationale         |
| Glossary                | Term definitions for domain-specific vocabulary                            |

## Clarifications

### Session 2026-02-01

- Q: How should the `docs/` directory be organized internally? → A: Minimal nesting — guides in `docs/` root, ADRs in `docs/adr/` subdirectory.
- Q: Should the documentation include worked examples with sample data? → A: Yes, one end-to-end worked example with a small synthetic dataset showing input, configuration, and all outputs.

## Assumptions

- The documentation will be maintained manually as part of the development process (no auto-generation from code)
- Markdown is the preferred format since the project is hosted on GitHub
- All documents will reside in a `docs/` directory at the repository root, with ADRs in a `docs/adr/` subdirectory
- Mathematical formulas will be written in LaTeX-compatible notation for GitHub rendering
- The target audience has intermediate Rust proficiency and basic statistics knowledge
- ADRs will follow the lightweight "Title / Context / Decision / Consequences" format
- Source code cross-references will use relative file paths with line numbers where applicable
- The CLAUDE.md file will be updated to reference the new documentation suite but will not be replaced by it

## Success Criteria

1. A new contributor can set up the project and submit a passing pull request by following only the documentation, without requiring synchronous help from existing maintainers.
2. All CLI arguments, TUI shortcuts, and configuration parameters are documented with descriptions, defaults, valid ranges, and behavioral effects.
3. Every statistical formula used in the codebase (WoE, IV, Gini, Pearson correlation, Welford algorithm, Laplace smoothing) is documented with the mathematical notation and edge-case handling rules.
4. All output file schemas (JSON, CSV, Parquet) have field-level documentation with data types and example values.
5. At least 5 Architectural Decision Records are created, covering the most significant technical choices in the project.
6. A data scientist can correctly interpret Lo-phi output files using only the documentation and glossary, without reading source code.
7. All documents are valid Markdown that renders correctly on GitHub without external tooling.
