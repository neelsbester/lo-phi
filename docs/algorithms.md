# Algorithm Reference

This document provides comprehensive technical specifications for all feature reduction algorithms implemented in Lo-phi. Each formula has been verified against the source code implementation.

## Overview

Lo-phi employs a three-stage feature selection pipeline that progressively eliminates features based on distinct criteria:

1. **Missing Value Analysis** - Removes features with excessive null values (default threshold: 30%)
2. **Information Value (IV) Analysis** - Removes features with low predictive power using Weight of Evidence (WoE) binning (default threshold: Gini < 0.05)
3. **Correlation Analysis** - Removes redundant features from highly correlated pairs using weighted Pearson correlation (default threshold: |r| > 0.40)

This sequential approach ensures that each stage operates on progressively refined feature sets. All stages support weighted samples, allowing for stratified sampling, class balancing, and importance weighting. The [architecture](architecture.md) document describes the module structure, while the [glossary](glossary.md) defines domain-specific terminology.

## Weight of Evidence (WoE) Binning

Weight of Evidence (WoE) is a transformation that quantifies the strength of a feature's relationship with a binary target. Lo-phi uses WoE as the foundation for both Information Value (IV) and Gini coefficient calculations.

### Mathematical Definition

For a bin containing event samples (target = 1) and non-event samples (target = 0), the WoE is calculated as:

$$\text{WoE} = \ln\left(\frac{\%_{\text{events}}}{\%_{\text{non-events}}}\right)$$

where:
- $\%_{\text{events}} = \frac{\text{events in bin}}{\text{total events in dataset}}$
- $\%_{\text{non-events}} = \frac{\text{non-events in bin}}{\text{total non-events in dataset}}$

### WoE Interpretation Convention

Lo-phi follows the `ln(%bad/%good)` convention where "bad" represents events (defaults/positives) and "good" represents non-events:

- **WoE > 0**: Bin has a higher event rate than the overall population (higher risk)
- **WoE < 0**: Bin has a lower event rate than the overall population (lower risk)
- **WoE ≈ 0**: Bin's event rate matches the overall population (neutral)

This convention is intuitive for credit scoring applications where higher WoE directly corresponds to higher default risk.

### Laplace Smoothing

To prevent undefined logarithms when a bin contains zero events or zero non-events, Lo-phi applies Laplace smoothing with constant `SMOOTHING = 0.5`:

$$\text{dist}_{\text{events}} = \frac{\text{events} + 0.5}{\text{total events} + 0.5}$$

$$\text{dist}_{\text{non-events}} = \frac{\text{non-events} + 0.5}{\text{total non-events} + 0.5}$$

$$\text{WoE} = \ln\left(\frac{\text{dist}_{\text{events}}}{\text{dist}_{\text{non-events}}}\right)$$

This smoothing ensures all WoE values are finite and well-defined, even for rare bins.

### Binning Strategies

Lo-phi implements two strategies for creating initial bins before merging:

#### CART (Decision Tree) Binning (Default)

CART binning uses recursive binary splitting based on Gini impurity reduction. For each potential split point, the algorithm calculates:

$$\text{Gini impurity} = 2p(1-p)$$

where $p = \frac{\text{events}}{\text{events} + \text{non-events}}$ is the event rate.

The **information gain** for a split is:

$$\text{Gain} = \text{Gini}_{\text{parent}} - \left(w_{\text{left}} \cdot \text{Gini}_{\text{left}} + w_{\text{right}} \cdot \text{Gini}_{\text{right}}\right)$$

where $w_{\text{left}}$ and $w_{\text{right}}$ are the weighted proportions of samples in each child node.

The algorithm selects the split point that maximizes information gain while respecting the minimum bin sample constraint (`MIN_BIN_SAMPLES = 5`). This process continues recursively until the target number of pre-bins (`DEFAULT_PREBINS = 20`) is reached or no valid splits remain.

#### Quantile (Equal-Frequency) Binning

Quantile binning divides the feature range into bins containing approximately equal weighted sample counts. For `DEFAULT_PREBINS = 20`, this creates bins at the 5th, 10th, 15th, ..., 95th weighted percentiles.

This strategy is simpler than CART and works well for features with uniform or near-uniform distributions.

### Pre-Binning and Merging

Both strategies begin with `DEFAULT_PREBINS = 20` initial bins. These are then merged to satisfy the constraint that each bin must contain at least `MIN_BIN_SAMPLES = 5` samples. The greedy merging algorithm:

1. Identifies bins with fewer than `MIN_BIN_SAMPLES` samples (raw count, not weighted)
2. Merges each small bin with its adjacent neighbor that yields the highest IV after merging
3. Repeats until all bins meet the minimum sample requirement

When solver-based optimization is enabled (see [Solver-Based Binning Optimization](#solver-based-binning-optimization)), these pre-bins are further merged to the target bin count (default: 10) using Mixed Integer Programming.

### Categorical Feature Handling

For categorical features (string columns, low-cardinality numerics), Lo-phi computes WoE separately for each category value:

1. **Rare Category Merging**: Categories with fewer than `DEFAULT_MIN_CATEGORY_SAMPLES = 5` samples are merged into an "OTHER" bin before WoE calculation
2. **CART-Based Merging**: When solver-based binning is enabled, categories are sorted by event rate and merged using the same CART approach as numeric features
3. **WoE Assignment**: Each category receives its own WoE value based on its event/non-event distribution

### Missing Value Handling

Missing values (null/NaN) are treated as a separate **MISSING** bin with its own WoE:

- Missing samples are excluded from regular binning
- A dedicated `MissingBin` is created containing all null-valued samples
- The MISSING bin's WoE is calculated using the same formula as regular bins
- Both IV and Gini calculations include the MISSING bin's contribution

This approach ensures that patterns in missingness (which can be predictive) are captured in the feature's overall IV and Gini scores.

### Error Handling

Lo-phi adheres to Constitution Principle 3 (fail loudly, never silently). When a feature cannot be binned due to:
- Insufficient non-missing samples
- All samples falling in a single category
- Extreme class imbalance (all events or all non-events)

...the analysis returns an error or warning rather than silently skipping the feature. Features that cannot be analyzed are reported in the reduction summary with appropriate diagnostic messages.

## Information Value (IV) Calculation

Information Value (IV) is a univariate measure of a feature's predictive power for binary classification. It quantifies how well a feature separates events from non-events.

### Formula

IV is the sum of IV contributions across all bins:

$$\text{IV} = \sum_{i=1}^{n} \left(\%_{\text{events}, i} - \%_{\text{non-events}, i}\right) \times \text{WoE}_i$$

where the sum runs over all bins (including the MISSING bin if present).

### Interpretation Thresholds

Lo-phi uses standard IV interpretation guidelines from credit scoring literature:

| IV Range | Predictive Power | Interpretation |
|----------|------------------|----------------|
| < 0.02 | Not predictive | Feature provides negligible separation |
| 0.02 - 0.1 | Weak | Feature has weak predictive power |
| 0.1 - 0.3 | Medium | Feature has moderate predictive power |
| 0.3 - 0.5 | Strong | Feature has strong predictive power |
| ≥ 0.5 | Very strong (suspect) | Feature may be overfit or leaking target information |

Features with IV ≥ 0.5 should be investigated for potential data leakage (e.g., the feature is derived from the target or represents future information).

### Weighted IV

When sample weights are provided, all counts in the IV formula are weighted sums rather than raw counts. This ensures IV reflects the importance-weighted predictive power rather than unweighted sample counts.

## Gini Coefficient

Lo-phi calculates the **Gini coefficient** (not to be confused with Gini impurity) as a measure of discriminatory power. The Gini coefficient is derived from the Area Under the ROC Curve (AUC) using WoE-encoded feature values.

### Relationship to AUC

The Gini coefficient is related to AUC by:

$$\text{Gini} = 2 \times \text{AUC} - 1$$

where AUC is the area under the ROC curve when the feature (after WoE encoding) is used as a score to rank samples.

### Weighted Mann-Whitney U Statistic

For weighted samples, Lo-phi calculates AUC using a weighted extension of the Mann-Whitney U statistic:

$$U = \sum_{\text{pos}} \text{weighted rank}_{\text{pos}} - \frac{\text{total pos weight}^2}{2}$$

$$\text{AUC} = \frac{U}{\text{total pos weight} \times \text{total neg weight}}$$

where:
- "pos" refers to positive class samples (target = 1)
- "weighted rank" is the cumulative weight of all samples with lower WoE plus half the weight of ties
- Total pos/neg weights are the sum of weights for positive/negative samples

### Algorithm Details

1. **WoE Encoding**: Each sample is assigned the WoE value of its bin (numeric features) or category (categorical features)
2. **Sorting**: Samples are sorted by WoE in ascending order
3. **Tie Handling**: Samples with identical WoE values are grouped; their weighted rank is the midpoint of their group's rank range
4. **AUC Calculation**: The weighted Mann-Whitney U statistic is computed and normalized
5. **Gini Transformation**: Gini = 2*AUC - 1

This ensures Gini ranges from -1 (perfect inverse discrimination) to +1 (perfect discrimination), with 0 indicating no discriminatory power.

### Gini Impurity (CART Splitting)

For CART binning, the **Gini impurity** is a different metric used to evaluate split quality:

$$\text{Gini impurity} = 2p(1-p)$$

where $p$ is the event rate. This measures node heterogeneity:
- Gini impurity = 0 for pure nodes (all one class)
- Gini impurity = 0.5 for maximum impurity (50/50 split)

Do not confuse Gini impurity (CART splitting criterion) with the Gini coefficient (discrimination measure).

## Pearson Correlation

Lo-phi identifies redundant features using weighted Pearson correlation. Pairs of features with correlation exceeding the threshold (default: 0.40) are flagged, and the feature appearing in more correlated pairs is dropped.

### Formula

The weighted Pearson correlation coefficient is:

$$r = \frac{\sum_i w_i (x_i - \bar{x}_w)(y_i - \bar{y}_w)}{\sqrt{\sum_i w_i (x_i - \bar{x}_w)^2} \cdot \sqrt{\sum_i w_i (y_i - \bar{y}_w)^2}}$$

where:
- $x_i, y_i$ are feature values for sample $i$
- $w_i$ is the weight for sample $i$
- $\bar{x}_w = \frac{\sum_i w_i x_i}{\sum_i w_i}$ is the weighted mean

### Welford's Algorithm for Numerical Stability

Lo-phi uses a single-pass weighted Welford algorithm to compute correlation coefficients without storing all data in memory. This incremental approach updates running statistics:

$$\bar{x}_{\text{new}} = \bar{x}_{\text{old}} + \frac{w}{\sum w} (x - \bar{x}_{\text{old}})$$

$$\text{Var}_x = \sum_i w_i (x_i - \bar{x}_w)(x_i - \bar{x}_w)$$

$$\text{Cov}_{xy} = \sum_i w_i (x_i - \bar{x}_w)(y_i - \bar{y}_w)$$

This algorithm is numerically stable for large datasets and avoids catastrophic cancellation errors that can occur with naive two-pass methods.

### Matrix Method vs Pairwise Computation

Lo-phi automatically selects the computation method based on the number of columns:

| Method | Used When | Complexity | Description |
|--------|-----------|------------|-------------|
| **Pairwise** | < 15 columns | $O(n^2 m)$ | Computes each correlation independently using Welford's algorithm |
| **Matrix** | ≥ 15 columns | $O(nm^2)$ | Computes full correlation matrix via $Z^T W Z$ where $Z$ is the standardized data matrix |

The threshold `MATRIX_METHOD_COLUMN_THRESHOLD = 15` was chosen because matrix multiplication becomes more efficient than $\binom{n}{2}$ pairwise computations when $n$ is large. Both methods produce identical results (within floating-point precision).

### Parallel Processing

Correlation computation is parallelized using Rayon:
- **Pairwise mode**: Each $(i, j)$ pair is computed independently in parallel
- **Matrix mode**: Column standardization is parallelized; matrix multiplication uses optimized BLAS-free faer library

This ensures efficient performance even on datasets with hundreds of features and millions of rows.

### Null Value Handling

Missing values in correlation analysis are handled by:
1. Excluding pairs where either feature is null from the weighted sums
2. Effectively treating nulls as "contributing zero weight" to the correlation
3. Reporting correlation only if sufficient non-null pairs exist

This differs from missing value handling in WoE binning, where nulls form their own MISSING bin.

## Missing Value Analysis

Missing value analysis computes the weighted proportion of null values for each feature.

### Formula

The weighted missing ratio for feature $f$ is:

$$\text{missing ratio}_f = \frac{\sum_{i: f_i \text{ is null}} w_i}{\sum_i w_i}$$

where $w_i$ is the weight for sample $i$.

### Threshold Interpretation

Features with missing ratio exceeding the threshold (default: 0.30) are dropped before IV/Gini analysis. This removes features with:
- Excessive data quality issues
- Sparse populations (e.g., optional fields completed by <70% of customers)
- Potential data collection problems

The weighted missing ratio ensures that importance-weighted samples (e.g., recent observations with higher weight) are properly accounted for when determining if a feature is too sparse to be useful.

## Solver-Based Binning Optimization

When solver-based binning is enabled (`--solver` CLI flag or TUI option), Lo-phi uses Mixed Integer Programming (MIP) to find globally optimal bin boundaries that maximize IV subject to constraints.

### MIP Model Formulation

Lo-phi formulates binning as a MIP problem with:

**Decision Variables:**
- $z_{i,j} \in \{0, 1\}$ for each potential bin spanning pre-bins $i$ through $j$ (inclusive)
- $z_{i,j} = 1$ if pre-bins $i..=j$ are merged into a single final bin

**Objective Function:**
$$\text{maximize} \quad \sum_{i=1}^{n} \sum_{j=i}^{n} \text{IV}_{i,j} \cdot z_{i,j}$$

where $\text{IV}_{i,j}$ is the pre-computed IV contribution of merging pre-bins $i$ through $j$.

**Constraints:**

1. **Bin Count Constraint**: Exactly $K$ bins are selected
   $$\sum_{i=1}^{n} \sum_{j=i}^{n} z_{i,j} = K$$

2. **Coverage Constraint**: Each pre-bin $p$ must be included in exactly one final bin
   $$\sum_{i=1}^{p} \sum_{j=p}^{n} z_{i,j} = 1 \quad \forall p \in \{1, \ldots, n\}$$

3. **Minimum Bin Size Constraint**: Only create variables for bins with $\geq$ `MIN_BIN_SAMPLES` samples (implicit in variable generation)

4. **Monotonicity Constraints** (optional): For adjacent bins $(i_1, j_1)$ and $(i_2, j_2)$ where $j_1 + 1 = i_2$:
   - **Ascending**: If $\text{WoE}_{i_1,j_1} > \text{WoE}_{i_2,j_2}$, add constraint $z_{i_1,j_1} + z_{i_2,j_2} \leq 1$
   - **Descending**: If $\text{WoE}_{i_1,j_1} < \text{WoE}_{i_2,j_2}$, add constraint $z_{i_1,j_1} + z_{i_2,j_2} \leq 1$

These constraints ensure that the selected bins form a valid partition of the pre-bins with WoE values following the specified monotonic pattern.

### Monotonicity Constraints

Lo-phi supports five monotonicity patterns:

| Constraint | Description | Use Case |
|------------|-------------|----------|
| **None** | No constraint on WoE pattern | Exploratory analysis, non-linear relationships |
| **Ascending** | WoE increases with feature value | Features where higher values = higher risk (e.g., debt-to-income ratio) |
| **Descending** | WoE decreases with feature value | Features where higher values = lower risk (e.g., credit score) |
| **Peak** | WoE increases then decreases | Features with optimal mid-range values |
| **Valley** | WoE decreases then increases | Features with extreme values indicating risk |
| **Auto** | Tries all patterns, selects best IV | Automatic pattern detection |

Monotonicity constraints are implemented as linear inequalities in the MIP model (see **Monotonicity Constraints** above).

### Solver Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `timeout_seconds` | 30 | Maximum solver time per feature |
| `gap_tolerance` | 0.01 | MIP optimality gap (1% = near-optimal solutions acceptable) |
| `min_bin_samples` | 5 | Minimum raw sample count per bin |

The solver (HiGHS via `good_lp`) terminates when:
- An optimal solution is found (gap = 0)
- The gap falls below `gap_tolerance`
- The timeout is reached

In practice, the solver typically finds optimal solutions within milliseconds for features with ≤20 pre-bins.

### Auto Mode: Trend Detection Heuristics

When `monotonicity = Auto`, Lo-phi:
1. Solves the MIP model independently for each of the five monotonicity patterns (None, Ascending, Descending, Peak, Valley)
2. Selects the solution with the highest total IV
3. Reports which monotonicity pattern was applied in the output

This automatic trend detection identifies the natural relationship between the feature and target without requiring domain knowledge.

## Constants Reference

All constants are defined in the source code and verified against implementation:

| Constant | Value | Location | Description |
|----------|-------|----------|-------------|
| `SMOOTHING` | 0.5 | `src/pipeline/iv.rs:25` | Laplace smoothing constant for WoE calculation |
| `MIN_BIN_SAMPLES` | 5 | `src/pipeline/iv.rs:22` | Minimum raw sample count per bin |
| `DEFAULT_PREBINS` | 20 | `src/pipeline/iv.rs:19` | Initial number of pre-bins before merging |
| `DEFAULT_MIN_CATEGORY_SAMPLES` | 5 | `src/pipeline/iv.rs:28` | Minimum samples per category before merging into OTHER |
| `MATRIX_METHOD_COLUMN_THRESHOLD` | 15 | `src/pipeline/correlation.rs:413` | Column count threshold for switching to matrix-based correlation |
| `TOLERANCE` | 1e-9 | `src/pipeline/target.rs:11` | Floating-point tolerance for binary target detection |
| Solver timeout | 30s | `src/pipeline/solver/mod.rs:35` | Default MIP solver timeout per feature |
| Solver gap | 0.01 | `src/pipeline/solver/mod.rs:36` | Default MIP optimality gap tolerance |

These constants represent the default configuration. Most can be overridden via CLI arguments (see `lo-phi --help` for details).
