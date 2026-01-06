---
title: Lo-phi (φ)
sub_title: Automated Feature Reduction for ML
author: Neels Bester
theme:
  name: dark
---

# Lo-phi (φ)

A **Rust CLI tool** for automated feature reduction in machine learning datasets

<!-- pause -->

Targets data scientists and ML engineers who need to reduce dimensionality before model training.

---

# The Problem

Machine learning datasets often suffer from:

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

## Too Many Features

- Curse of dimensionality
- Overfitting risk
- Slow training times

<!-- column: 1 -->

## Low-Quality Features

- Missing values
- No predictive power
- Redundant information

<!-- reset_layout -->

---

# Three Reduction Strategies

<!-- pause -->

## 1️⃣ Missing Value Analysis
Removes features with excessive missing data (>30%)

<!-- pause -->

## 2️⃣ Univariate Gini Analysis  
Removes features with low predictive power via WoE binning

<!-- pause -->

## 3️⃣ Correlation Analysis
Removes redundant features from highly correlated pairs

---

# Technology Stack

| Component | Library | Purpose |
|-----------|---------|---------|
| Data | `polars` | Memory-efficient streaming |
| CLI | `clap` | Type-safe arguments |
| Parallel | `rayon` | Multi-threaded processing |
| TUI | `ratatui` | Interactive config |
| Progress | `indicatif` | Visual progress bars |

---

# Basic Usage

```bash
# Interactive mode
lophi --input data.csv

# With target column specified
lophi --input data.csv --target target_column --output reduced.parquet

# Non-interactive with all options
lophi --input data.parquet \
  --target target \
  --missing-threshold 0.3 \
  --gini-threshold 0.05 \
  --correlation-threshold 0.95 \
  --no-confirm
```

---

# Project Structure

```
lophi/
├── src/
│   ├── main.rs          # Entry point
│   ├── cli/             # Clap args & TUI menu
│   ├── pipeline/        # Core reduction logic
│   │   ├── loader.rs    # CSV/Parquet loading
│   │   ├── missing.rs   # Missing value analysis
│   │   ├── iv.rs        # Gini/IV calculation
│   │   └── correlation.rs
│   └── report/          # Summary & JSON export
└── tests/               # Integration tests
```

---

# Key Implementation Details

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

## Memory Efficiency
- Polars LazyFrame optimization
- Streaming CSV→Parquet
- Chunk-based correlation

<!-- column: 1 -->

## Gini Calculation
- 50 initial quantile bins
- Greedy merging algorithm
- Laplace smoothing for WoE
- AUC via Mann-Whitney U

<!-- reset_layout -->

---

# Example Output

```
    ✦ REDUCTION SUMMARY
    ──────────────────────────────────────────────────────
    │ Metric                │ Value │
    │ ❮ Initial Features    │ 69    │
    │ ✗ Dropped (Missing)   │ 3     │
    │ ◈ Dropped (Low Gini)  │ 12    │
    │ ⋈ Dropped (Correlation)│ 5     │
    │ ✓ Final Features      │ 49    │
    │ ↓ Reduction           │ 29.0% │
```

---

# Thank You!

**Lo-phi (φ)** - Feature reduction made simple

🦀 Built with Rust for performance and reliability

<!-- end_slide -->

