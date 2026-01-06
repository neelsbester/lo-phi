# Lo-phi (φ)
## Automated Feature Reduction for ML

A Rust CLI tool that streamlines feature engineering

---

# The Problem

Machine learning datasets often have:

* 🗑️  Features with too many missing values
* 📉  Low-predictive-power features  
* 🔗  Highly correlated (redundant) features

**Lo-phi solves this automatically.**

---

# Three Reduction Strategies

```
┌─────────────────────────────────────────┐
│  1. Missing Value Analysis              │
│     → Removes features > 30% missing    │
├─────────────────────────────────────────┤
│  2. Univariate Gini Analysis            │
│     → Removes low predictive features   │
├─────────────────────────────────────────┤
│  3. Correlation Analysis                │
│     → Removes redundant features        │
└─────────────────────────────────────────┘
```

---

# Technology Stack

| Component         | Library              |
|-------------------|----------------------|
| Data Processing   | Polars (streaming)   |
| CLI               | Clap                 |
| Parallel          | Rayon                |
| TUI               | Ratatui              |
| Progress          | Indicatif            |

---

# Basic Usage

~~~bash
# Interactive mode
lophi --input data.csv

# With target column
lophi --input data.csv --target target_column

# Full options
lophi --input data.parquet \
  --target target \
  --missing-threshold 0.3 \
  --gini-threshold 0.05 \
  --correlation-threshold 0.95
~~~

---

# Live Demo

~~~bash
lophi --input test_data/small_test.parquet --target target
~~~

---

# Key Features

* ⚡ **Memory Efficient** - Polars LazyFrame streaming
* 🔄 **Parallel Processing** - Rayon multi-threading  
* 📊 **Gini/IV Export** - JSON analysis output
* 🎨 **Interactive TUI** - Ratatui config menu
* 📁 **CSV ↔ Parquet** - Format conversion

---

# Example Output

```
    ✦ REDUCTION SUMMARY
    ──────────────────────────────────
    │ ❮ Initial Features    │ 69    │
    │ ✗ Dropped (Missing)   │ 3     │
    │ ◈ Dropped (Low Gini)  │ 12    │
    │ ⋈ Dropped (Correlation)│ 5     │
    │ ✓ Final Features      │ 49    │
    │ ↓ Reduction           │ 29.0% │
```

---

# Thank You!

**Lo-phi** - Making feature reduction simple

```
  github.com/neelsbester/lo-phi
```

