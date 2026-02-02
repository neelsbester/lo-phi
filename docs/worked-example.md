# Worked Example: End-to-End Feature Reduction Pipeline

This document walks through a complete Lo-phi feature reduction pipeline using a synthetic dataset with actual output. This example demonstrates all three reduction stages (missing value analysis, Gini/IV analysis, and correlation analysis) with real results that you can reproduce.

## Introduction

This worked example demonstrates Lo-phi's complete feature reduction pipeline on a small synthetic dataset designed to showcase each stage of the analysis. The dataset contains five features with carefully constructed properties that trigger each reduction criterion:

1. A feature with excessive missing values (dropped at missing stage)
2. A feature with zero predictive power (dropped at Gini/IV stage)
3. Two highly correlated features (both dropped at correlation stage)
4. One useful feature that survives all stages

By the end of this walkthrough, you will understand how Lo-phi processes features sequentially through each stage and how to interpret the output reports.

## Synthetic Dataset

The synthetic dataset (`docs/examples/synthetic_data.csv`) contains 20 rows with 5 features plus 1 binary target column. The dataset was designed to demonstrate each reduction stage:

**Dataset Structure:**
- **Rows:** 20 samples
- **Features:** 5 (3 numeric, 2 categorical)
- **Target:** Binary (0/1) with 8 events and 12 non-events
- **Event rate:** 40% (8/20)

**Feature Descriptions:**

1. **`income`** (numeric) - Ranges from 12,000 to 85,000. High [Information Value](glossary.md#information-value-iv) (perfectly separates events from non-events). Strongly correlated with `age`.

2. **`age`** (numeric) - Ranges from 20 to 55. High IV (identical separation pattern to `income`). Strongly correlated with `income` (r=0.99).

3. **`debt_ratio`** (numeric) - Debt-to-income ratio. **45% missing values** (9 out of 20 rows are null), designed to trigger the missing value threshold.

4. **`employment`** (categorical) - Three categories: "employed" (10 samples), "unemployed" (6 samples), "self_employed" (4 samples). High IV due to strong association with target (unemployed individuals have high event rate).

5. **`region`** (categorical) - Four categories: "north", "south", "east", "west" (5 samples each). **Zero predictive power** - events are evenly distributed across all regions, designed to trigger the Gini threshold.

**First 10 rows of the dataset:**

```csv
income,age,debt_ratio,employment,region,target
85000,55,0.25,employed,north,0
72000,48,,employed,south,0
68000,45,0.30,self_employed,east,0
63000,42,0.35,employed,west,0
58000,40,,employed,north,0
52000,38,0.40,employed,south,0
48000,35,,self_employed,east,0
45000,33,0.28,employed,west,0
40000,32,,employed,north,0
78000,50,0.22,employed,south,0
```

Notice that `debt_ratio` has many missing values (blank cells), and the target column shows a clear split: higher income/age values correspond to target=0, lower values to target=1.

## Configuration

The analysis was run using the following CLI command:

```bash
lophi --input docs/examples/synthetic_data.csv --target target \
  --missing-threshold 0.30 --gini-threshold 0.05 --correlation-threshold 0.40 \
  --binning-strategy cart --no-confirm
```

**Parameter Breakdown:**

- **`--input`**: Input CSV file path
- **`--target target`**: The binary target column name
- **`--missing-threshold 0.30`**: Drop features with >30% missing values (see [Missing Value Analysis](algorithms.md#missing-value-analysis))
- **`--gini-threshold 0.05`**: Drop features with [Gini coefficient](glossary.md#gini-coefficient) <0.05 (see [Gini/IV Analysis](algorithms.md#gini-coefficient))
- **`--correlation-threshold 0.40`**: Drop one feature from pairs with |r| >0.40 (see [Pearson Correlation](algorithms.md#pearson-correlation))
- **`--binning-strategy cart`**: Use decision-tree based [binning](glossary.md#binning) for optimal splits
- **`--no-confirm`**: Skip interactive confirmation prompts

These are the default thresholds recommended for most use cases. See the [User Guide](user-guide.md#configuration-parameters) for details on how each parameter affects the pipeline.

## Pipeline Walkthrough

Lo-phi processes features through three sequential stages. Each stage only sees features that survived the previous stage. Let's examine each stage in detail using actual output from the reduction reports.

### Stage 1: Missing Value Analysis

The first stage calculates the [null ratio](glossary.md#null-ratio) (proportion of missing values) for each feature. Features exceeding the 0.30 threshold are immediately dropped.

**Result:** `debt_ratio` was dropped

The `debt_ratio` feature had 9 null values out of 20 rows, resulting in a missing ratio of 0.45 (45%), which exceeds the 0.30 threshold.

From `synthetic_data_reduction_report.json`:

```json
{
  "name": "debt_ratio",
  "status": "dropped",
  "dropped_at_stage": "missing",
  "reason": "Missing ratio 0.45 exceeded threshold 0.30",
  "analysis": {
    "missing": {
      "ratio": 0.45,
      "threshold": 0.3,
      "passed": false
    }
  }
}
```

**Interpretation:** The weighted [null ratio](glossary.md#null-ratio) is calculated as:

$$\text{missing ratio} = \frac{\text{null count}}{\text{total count}} = \frac{9}{20} = 0.45$$

Since 0.45 > 0.30, the feature is dropped. Features with excessive missing values are unreliable for modeling and are removed before more expensive IV/correlation analyses.

**Remaining features after Stage 1:** `income`, `age`, `employment`, `region` (4 features)

### Stage 2: Gini/IV Analysis

The second stage performs [Weight of Evidence (WoE)](glossary.md#weight-of-evidence-woe) binning for each remaining feature and calculates their [Information Value (IV)](glossary.md#information-value-iv) and [Gini coefficient](glossary.md#gini-coefficient). Features with Gini below 0.05 are dropped.

**Result:** `region` was dropped (Gini=0.0, IV=0.0)

The `region` feature showed zero predictive power. All four region categories ("east", "north", "south", "west") were merged into a single bin because they all had the same event rate (0.4, matching the overall population event rate).

From `synthetic_data_gini_analysis.json`:

```json
{
  "feature_name": "region",
  "feature_type": "Categorical",
  "categories": [
    {
      "categories": [
        "east",
        "north",
        "south",
        "west"
      ],
      "events": 8.0,
      "non_events": 12.0,
      "woe": 0.0,
      "iv_contribution": 0.0,
      "count": 20.0,
      "population_pct": 100.0,
      "event_rate": 0.4
    }
  ],
  "iv": 0.0,
  "gini": 0.0,
  "dropped": true
}
```

**Interpretation:** The [WoE](glossary.md#weight-of-evidence-woe) is calculated as:

$$\text{WoE} = \ln\left(\frac{\%_{\text{events}}}{\%_{\text{non-events}}}\right)$$

When all categories have the same event rate as the overall population, the distribution percentages for events and non-events are equal, resulting in WoE = ln(1) = 0. With WoE = 0 for all bins, the [IV](glossary.md#information-value-iv) contribution is:

$$\text{IV} = \sum (\%_{\text{events}} - \%_{\text{non-events}}) \times \text{WoE} = 0$$

Since Gini = 0.0 < 0.05, the feature is dropped.

**Surviving Features:**

The three features that passed the Gini threshold show strong predictive power:

From `synthetic_data_reduction_report.json`:

| Feature | Type | IV | Gini |
|---------|------|-----|------|
| `income` | Numeric | 5.7567 | 1.0 |
| `age` | Numeric | 5.7567 | 1.0 |
| `employment` | Categorical | 3.0382 | 0.8125 |

**WoE Bins for `income`:**

The `income` feature was split into two bins by the CART algorithm, achieving perfect separation (Gini = 1.0):

From `synthetic_data_gini_analysis.json`:

```json
{
  "feature_name": "income",
  "feature_type": "Numeric",
  "bins": [
    {
      "lower_bound": 12000.0,
      "upper_bound": 40000.0,
      "events": 8.0,
      "non_events": -0.0,
      "woe": 3.2188758248682006,
      "iv_contribution": 3.0901207918734723,
      "count": 8.0,
      "population_pct": 40.0,
      "event_rate": 1.0
    },
    {
      "lower_bound": 40000.0,
      "upper_bound": null,
      "events": -0.0,
      "non_events": 12.0,
      "woe": -2.833213344056216,
      "iv_contribution": 2.6665537355823212,
      "count": 12.0,
      "population_pct": 60.0,
      "event_rate": -0.0
    }
  ],
  "iv": 5.756674527455793,
  "gini": 1.0
}
```

**Interpretation:**

- **Bin 1** [12,000 - 40,000): All 8 events (target=1), zero non-events. WoE = +3.22 (high risk).
- **Bin 2** [40,000 - infinity): All 12 non-events (target=0), zero events. WoE = -2.83 (low risk).

The perfect separation (event_rate = 1.0 in bin 1, event_rate = 0.0 in bin 2) results in Gini = 1.0, indicating perfect discriminatory power. The extremely high IV (5.76) suggests potential data leakage or perfect separation, which should be investigated in real-world datasets (see [IV interpretation](algorithms.md#information-value-iv-calculation)).

**WoE Categories for `employment`:**

The `employment` feature had three categories, with "self_employed" merged into "OTHER" due to having fewer than the minimum required samples:

From `synthetic_data_gini_analysis.json`:

```json
{
  "feature_name": "employment",
  "feature_type": "Categorical",
  "categories": [
    {
      "categories": [
        "employed"
      ],
      "events": 1.0,
      "non_events": 9.0,
      "woe": -1.460164209686346,
      "iv_contribution": 0.8520487623581502,
      "count": 10.0,
      "population_pct": 50.0,
      "event_rate": 0.1
    },
    {
      "category": "OTHER",
      "events": 1.0,
      "non_events": 3.0,
      "woe": -0.461635379575219,
      "iv_contribution": 0.04779283929719915,
      "count": 4.0,
      "population_pct": 20.0,
      "event_rate": 0.25
    },
    {
      "categories": [
        "unemployed"
      ],
      "events": 6.0,
      "non_events": 0.0,
      "woe": 2.9506118382735216,
      "iv_contribution": 2.138325755737046,
      "count": 6.0,
      "population_pct": 30.0,
      "event_rate": 1.0
    }
  ],
  "iv": 3.0381673573923953,
  "gini": 0.8125
}
```

**Interpretation:**

- **employed**: 1 event, 9 non-events → event_rate = 0.1 (low risk, WoE = -1.46)
- **OTHER** (self_employed): 1 event, 3 non-events → event_rate = 0.25 (moderate risk, WoE = -0.46)
- **unemployed**: 6 events, 0 non-events → event_rate = 1.0 (high risk, WoE = +2.95)

The `employment` feature has strong predictive power (IV = 3.04, Gini = 0.81) with a clear pattern: unemployed individuals have much higher event rates than employed individuals.

**Remaining features after Stage 2:** `income`, `age`, `employment` (3 features)

### Stage 3: Correlation Analysis

The third stage calculates [Pearson correlation](glossary.md#pearson-correlation) between all numeric features (categorical features like `employment` are excluded from correlation analysis). Pairs with |r| > 0.40 are identified, and the feature appearing in more correlated pairs is dropped.

**Result:** Both `income` and `age` were dropped

The `income` and `age` features were highly correlated with each other (r=0.9906) **and** both were correlated with the target column:

From `synthetic_data_reduction_report.json`:

```json
{
  "name": "age",
  "status": "dropped",
  "dropped_at_stage": "correlation",
  "reason": "Correlated with income (r=0.9906), dropped due to higher correlation frequency",
  "analysis": {
    "correlation": {
      "max_correlation": 0.9906041144212815,
      "correlated_with": "income",
      "threshold": 0.4,
      "passed": false,
      "all_correlations": [
        {
          "feature": "income",
          "correlation": 0.9906041144212815
        },
        {
          "feature": "target",
          "correlation": -0.8043881978311005
        }
      ]
    }
  }
}
```

```json
{
  "name": "income",
  "status": "dropped",
  "dropped_at_stage": "correlation",
  "reason": "Correlated with age (r=0.9906), dropped due to higher correlation frequency",
  "analysis": {
    "correlation": {
      "max_correlation": 0.9906041144212815,
      "correlated_with": "age",
      "threshold": 0.4,
      "passed": false,
      "all_correlations": [
        {
          "feature": "age",
          "correlation": 0.9906041144212815
        },
        {
          "feature": "target",
          "correlation": -0.8649006347414253
        }
      ]
    }
  }
}
```

**Interpretation:**

The correlation matrix shows:

| Pair | Correlation |
|------|-------------|
| income ↔ age | +0.9906 |
| income ↔ target | -0.8649 |
| age ↔ target | -0.8044 |

Both `income` and `age` participate in **2 correlated pairs each** (income-age and income-target for `income`; age-income and age-target for `age`). When the correlation frequency is tied, both features are dropped. This is intentional behavior when features are redundant with each other **and** with the target.

The negative correlation with target (-0.86 and -0.80) indicates that higher income/age values correspond to lower target values (target=0), which aligns with the WoE bin patterns observed earlier.

**Note on target correlation:** Lo-phi includes the target column in correlation analysis by default. This can cause highly predictive features to be dropped if they are correlated with both the target and other features. See [Pearson Correlation](algorithms.md#pearson-correlation) for details.

**Note on categorical features:** The `employment` feature (categorical) was excluded from correlation analysis. Lo-phi only computes [Pearson correlation](glossary.md#pearson-correlation) for numeric features. Future versions may support [Cramér's V](glossary.md#cramers-v) for categorical correlation (see CLAUDE.md TODO).

**Remaining features after Stage 3:** `employment` (1 feature)

## Results Summary

The complete reduction pipeline transformed the dataset from 5 features to 1 feature, removing 80% of the original features.

**Reduction Breakdown:**

| Stage | Features In | Features Out | Dropped | Reason |
|-------|-------------|--------------|---------|--------|
| Initial | - | 5 | - | - |
| Missing | 5 | 4 | 1 | `debt_ratio` (45% missing) |
| Gini/IV | 4 | 3 | 1 | `region` (Gini=0.0) |
| Correlation | 3 | 1 | 2 | `income`, `age` (r=0.99) |
| **Final** | - | **1** | **4** | - |

**Timing Breakdown:**

From `synthetic_data_reduction_report.json`:

```json
"timing": {
  "load_ms": 3,
  "missing_ms": 0,
  "gini_ms": 1,
  "correlation_ms": 0,
  "save_ms": 0,
  "total_ms": 6
}
```

The entire pipeline completed in 6 milliseconds due to the small dataset size (20 rows). For larger datasets, Gini/IV analysis is typically the slowest stage due to [WoE binning](glossary.md#binning) computation.

**Reduction Summary from CSV Report:**

The complete summary from `synthetic_data_reduction_report.csv`:

```csv
feature,status,dropped_at_stage,reason,missing_ratio,gini,iv,feature_type,max_correlation,correlated_with
employment,kept,,,-0.0000,0.8125,3.0382,Categorical,,
debt_ratio,dropped,missing,Missing ratio 0.45 exceeded threshold 0.30,0.4500,,,,,
region,dropped,gini,Gini coefficient 0.0000 below threshold 0.0500,-0.0000,0.0000,0.0000,Categorical,,
age,dropped,correlation,"Correlated with income (r=0.9906), dropped due to higher correlation frequency",-0.0000,1.0000,5.7567,Numeric,0.9906,"income: 0.9906 | target: -0.8044"
income,dropped,correlation,"Correlated with age (r=0.9906), dropped due to higher correlation frequency",-0.0000,1.0000,5.7567,Numeric,0.9906,"age: 0.9906 | target: -0.8649"
```

This human-readable CSV format shows all five features with their analysis results. The `correlated_with` column uses pipe-separated format to list all correlated features (see [Output Reference](output-reference.md#correlated-with-format) for schema details).

## Output Files

Lo-phi generated the following output files in the `docs/examples/` directory:

### 1. Reduced Dataset

**File:** `synthetic_data_reduced.csv`

**Content:** The final dataset with 20 rows and 2 columns (`employment` and `target`):

```csv
employment,target
employed,0
employed,0
self_employed,0
employed,0
employed,0
employed,0
self_employed,0
employed,0
employed,0
employed,0
employed,0
self_employed,0
unemployed,1
unemployed,1
unemployed,1
employed,1
unemployed,1
unemployed,1
self_employed,1
unemployed,1
```

The reduced dataset retains:
- The surviving feature: `employment`
- The target column: `target` (always retained)
- All 20 rows in original order
- Original data types and values (no imputation or transformation)

### 2. Reduction Report Bundle

**File:** `synthetic_data_reduction_report.zip`

**Contents:**
1. **`synthetic_data_reduction_report.json`** (1.7 KB) - Full JSON report with metadata, summary, and per-feature analysis. See [Reduction Report JSON](output-reference.md#reduction-report-json) for schema.

2. **`synthetic_data_gini_analysis.json`** (1.5 KB) - Detailed WoE/IV binning results for all features analyzed during the Gini stage. See [Gini Analysis JSON](output-reference.md#gini-analysis-json) for schema.

3. **`synthetic_data_reduction_report.csv`** (400 bytes) - Human-readable summary table shown above. See [Reduction Report CSV](output-reference.md#reduction-report-csv) for schema.

**To extract:**

```bash
unzip synthetic_data_reduction_report.zip
```

The ZIP bundle uses standard Deflate compression and can be opened with any ZIP utility or archive manager. See [Output Reference](output-reference.md#reduction-report-zip-bundle) for packaging details.

## Key Takeaways

### 1. Sequential Pipeline

Lo-phi processes features through three sequential stages. Each stage only sees features that survived the previous stage:

- Missing → Gini → Correlation
- `debt_ratio` was dropped at Stage 1, so it never reached Gini/IV analysis
- `region` was dropped at Stage 2, so it never reached correlation analysis

This design ensures efficient processing: expensive analyses (WoE binning, correlation) are only performed on features that passed cheaper filters.

### 2. Small Sample Behavior

With only 20 rows, features can exhibit extreme statistical properties:

- **Perfect separation:** `income` and `age` achieved Gini = 1.0 (perfect discrimination)
- **Extreme IV:** IV = 5.76 is suspiciously high (>0.5 indicates potential data leakage in real datasets)
- **Zero predictive power:** `region` had exactly equal event rates across all categories

In real-world datasets with thousands of rows, such extreme values are rare and warrant investigation. See [IV interpretation thresholds](output-reference.md#information-value-thresholds) for guidance.

### 3. Target Correlation

The correlation analysis includes the target column, which can cause highly predictive features to be dropped if they are correlated with both the target and other features.

In this example:
- `income` was correlated with `age` (r=0.99) and `target` (r=-0.86)
- `age` was correlated with `income` (r=0.99) and `target` (r=-0.80)
- Both features participated in 2 correlated pairs, so both were dropped

If you want to preserve features highly correlated with the target, consider increasing `--correlation-threshold` or manually reviewing the correlation matrix before dropping features.

### 4. Categorical Feature Handling

Categorical features are excluded from numeric [Pearson correlation](glossary.md#pearson-correlation) analysis. In this example, `employment` (categorical) was the only feature to survive because:

- It had zero missing values (passed Stage 1)
- It had high Gini/IV (0.81 / 3.04, passed Stage 2)
- It was excluded from correlation analysis (automatic pass for categorical features)

Future versions may support [Cramér's V](glossary.md#cramers-v) for categorical correlation analysis (see CLAUDE.md TODO).

### 5. Report Interpretation

The reduction reports provide multiple views of the same analysis:

- **JSON reports** for programmatic access and detailed bin-level information
- **CSV summary** for quick human review in spreadsheet applications
- **Reduced dataset** for downstream modeling with low-value features removed

Always review the CSV summary first to understand which features were dropped and why. For features with unexpected Gini/IV values, drill into the JSON reports to examine WoE bins and event rate distributions.

See the [Output Reference](output-reference.md) for complete schema documentation and interpretation guidelines.

---

This worked example demonstrates the complete Lo-phi pipeline with real output data. For more information on the underlying algorithms, see the [Algorithm Reference](algorithms.md). For CLI usage and configuration options, see the [User Guide](user-guide.md). For term definitions, see the [Glossary](glossary.md).
