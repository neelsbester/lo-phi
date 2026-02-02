# Glossary

This glossary defines domain-specific terminology used throughout the Lo-phi project. Terms are listed alphabetically with definitions, context within Lo-phi, related concepts, and relevant formulas. Understanding these terms is essential for working with Lo-phi's feature reduction pipeline.

---

### Bad Rate

**Definition:** The proportion of events (target = 1) within a specific bin or population segment. Also called "event rate" or "default rate" in credit scoring contexts.

**Context in Lo-phi:** Calculated for each bin during WoE analysis as `events / (events + non_events)`. Used to compute distribution percentages for WoE calculation and to sort categorical bins in CART binning strategy.

**Related Terms:** Event Rate, Good Rate, Weight of Evidence

**Formula:**
$$\text{Bad Rate} = \frac{\text{events}}{\text{events} + \text{non\_events}}$$

---

### AUC (Area Under the ROC Curve)

**Definition:** A metric measuring the probability that a model ranks a randomly chosen event higher than a randomly chosen non-event. Ranges from 0.5 (random) to 1.0 (perfect discrimination).

**Context in Lo-phi:** Used internally to calculate the Gini coefficient via `Gini = 2 * AUC - 1`. AUC is computed using a weighted Mann-Whitney U statistic over WoE-encoded feature values in `src/pipeline/iv.rs`.

**Related Terms:** Gini Coefficient, Weight of Evidence

---

### Binning

**Definition:** The process of grouping continuous numeric values or categorical values into discrete intervals (bins) to analyze their relationship with a binary target variable.

**Context in Lo-phi:** Core step in IV analysis implemented in `src/pipeline/iv.rs`. Lo-phi supports two binning strategies: quantile (equal-frequency) and CART (decision-tree based). Default prebinning uses 20 bins before merging.

**Related Terms:** Prebinning, Quantile Binning, CART, Weight of Evidence

---

### CART (Classification and Regression Trees)

**Definition:** A decision-tree algorithm that creates binary splits by maximizing information gain at each node. In binning context, it groups values to maximize separation between event and non-event populations.

**Context in Lo-phi:** Available as `BinningStrategy::Cart` (default strategy). Produces bins with maximum discriminatory power by finding optimal split points. Configured via `--binning-strategy cart` CLI flag and `--cart-min-bin-pct` parameter (default 5.0%).

**Related Terms:** Binning, Quantile Binning, Information Value

---

### Cramer's V

**Definition:** A measure of association between two categorical variables, normalized to range [0, 1]. Based on chi-squared statistic.

**Context in Lo-phi:** PLANNED FEATURE (not yet implemented). Will enable correlation analysis for categorical features using Cramér's V instead of Pearson correlation. Documented in CLAUDE.md TODOs as future enhancement.

**Related Terms:** Pearson Correlation, Feature Reduction

**Formula:**
$$V = \sqrt{\frac{\chi^2}{N \times (k - 1)}}$$
where $k = \min(\text{categories}_A, \text{categories}_B)$

---

### Event Rate

**Definition:** Synonym for "bad rate." The proportion of events (target = 1) in a bin or population.

**Context in Lo-phi:** Stored in `WoeBin.event_rate` and `CategoricalWoeBin.event_rate` fields. Calculated as `events / count` where count is the weighted total samples in the bin.

**Related Terms:** Bad Rate, Good Rate, Weight of Evidence

---

### Feature Reduction

**Definition:** The process of systematically removing features (columns) from a dataset based on statistical criteria to improve model efficiency and reduce overfitting.

**Context in Lo-phi:** The core purpose of Lo-phi. Implements three reduction stages: (1) null ratio threshold, (2) Gini/IV threshold, (3) correlation threshold. Each stage removes features and produces a comprehensive report.

**Related Terms:** Null Ratio, Information Value, Gini Coefficient, Pearson Correlation

---

### Gap Tolerance

**Definition:** The MIP (Mixed-Integer Programming) optimality gap threshold at which the solver terminates. Gap represents the relative difference between the best solution found and the theoretical upper bound.

**Context in Lo-phi:** Configured via `SolverConfig.gap_tolerance` with default value 0.01 (1%). Solver stops when gap falls below this threshold or timeout is reached. Configurable via `--solver-gap` CLI parameter.

**Related Terms:** Solver, MIP

**Formula:**
$$\text{Gap} = \frac{\text{Upper Bound} - \text{Best Solution}}{\text{Best Solution}}$$

---

### Gini Coefficient

**Definition:** A measure of predictive power derived from Information Value, representing the area between the Lorenz curve and the diagonal. Ranges from 0 (no discrimination) to 1 (perfect discrimination).

**Context in Lo-phi:** Calculated for each feature during IV analysis. Features with Gini below the threshold (default 0.05) are dropped. Exported to `{input}_gini_analysis.json` in the reduction report.

**Related Terms:** Information Value, Weight of Evidence

**Formula:**
$$\text{Gini} = 2 \times \text{AUC} - 1$$
where AUC is derived from IV-based score separation.

---

### Gini Impurity

**Definition:** A measure of node impurity used in decision tree algorithms, calculated as `2 * p * (1 - p)` where `p` is the event proportion. Maximized at 0.5 (equal split) and minimized at 0 (pure node). Not to be confused with the Gini coefficient.

**Context in Lo-phi:** Used in CART binning to find optimal split points. At each candidate split, Gini impurity is calculated for both resulting child nodes, and the split maximizing information gain (reduction in weighted impurity) is selected. Implemented in `src/pipeline/iv.rs`.

**Related Terms:** CART, Gini Coefficient, Binning

**Formula:**
$$\text{Gini Impurity} = 2 \times p \times (1 - p)$$

---

### Good Rate

**Definition:** The proportion of non-events (target = 0) within a specific bin or population segment. The complement of bad rate.

**Context in Lo-phi:** Implicitly used in WoE calculation as `non_events / (events + non_events)`. Not stored directly but derivable from event rate.

**Related Terms:** Bad Rate, Event Rate, Weight of Evidence

**Formula:**
$$\text{Good Rate} = 1 - \text{Bad Rate} = \frac{\text{non\_events}}{\text{events} + \text{non\_events}}$$

---

### Information Value (IV)

**Definition:** A statistical measure quantifying the predictive strength of a feature relative to a binary target. Sum of IV contributions across all bins, where each contribution is the product of WoE and the difference in event/non-event distributions.

**Context in Lo-phi:** Primary metric for feature selection in `src/pipeline/iv.rs`. Features with IV below the Gini threshold (default 0.05) are dropped. Total IV is maximized by the MIP solver when solver mode is enabled.

**Related Terms:** Weight of Evidence, Gini Coefficient, Binning

**Formula:**
$$\text{IV} = \sum_{i=1}^{n} (\%\text{events}_i - \%\text{non\_events}_i) \times \text{WoE}_i$$

---

### Laplace Smoothing

**Definition:** A technique to prevent division by zero or logarithm of zero by adding a small constant (smoothing factor) to counts before calculation.

**Context in Lo-phi:** Applied in WoE calculation with `SMOOTHING = 0.5`. Defined as constant in `src/pipeline/iv.rs` line 25. Ensures stable WoE estimates even for bins with zero events or non-events.

**Related Terms:** Weight of Evidence, Information Value

**Formula:**
$$\%\text{events} = \frac{\text{events} + 0.5}{\text{total\_events} + 0.5}$$

---

### MIP (Mixed-Integer Programming)

**Definition:** An optimization technique that solves problems with both continuous and integer decision variables. Used to find globally optimal solutions subject to constraints.

**Context in Lo-phi:** Powers the solver-based optimal binning in `src/pipeline/solver/`. Uses HiGHS solver via `good_lp` crate to maximize IV while respecting bin count and monotonicity constraints. Enabled by default, configurable via `--use-solver` flag.

**Related Terms:** Solver, Gap Tolerance, Monotonicity Constraint

---

### Monotonicity Constraint

**Definition:** A requirement that Weight of Evidence values follow a specific pattern (ascending, descending, peak, valley) across bins ordered by feature value.

**Context in Lo-phi:** Enforced during solver-based binning via `MonotonicityConstraint` enum in `src/pipeline/solver/monotonicity.rs`. Options: none (default), ascending, descending, peak, valley, auto. Important for regulatory compliance and model interpretability. Configured via `--trend` CLI parameter.

**Related Terms:** Weight of Evidence, Solver, MIP

---

### Null Ratio

**Definition:** The proportion of missing (null) values in a feature column. For weighted analysis, the weighted null count divided by total weight.

**Context in Lo-phi:** First reduction stage in `src/pipeline/missing.rs`. Features exceeding the null ratio threshold (default 0.30) are dropped. Calculated as `weighted_null_count / total_weight` when weights are provided.

**Related Terms:** Feature Reduction, Weighted Analysis

**Formula:**
$$\text{Null Ratio} = \frac{\sum_{i} w_i \cdot \mathbb{1}(\text{value}_i = \text{null})}{\sum_{i} w_i}$$

---

### Pearson Correlation

**Definition:** A measure of linear association between two numeric variables, ranging from -1 (perfect negative correlation) to +1 (perfect positive correlation).

**Context in Lo-phi:** Used in `src/pipeline/correlation.rs` to identify redundant features. Pairs with correlation above threshold (default 0.40) trigger feature removal. Implements weighted Pearson correlation using the Welford algorithm for numerical stability.

**Related Terms:** Welford Algorithm, Feature Reduction, Weighted Analysis

**Formula:**
$$r = \frac{\text{Cov}(X, Y)}{\sigma_X \sigma_Y}$$
For weighted correlation: weights are incorporated into covariance and standard deviation calculations.

---

### Population Splitting

**Definition:** Dividing a dataset into development (training) and validation subsets, typically for model building and testing.

**Context in Lo-phi:** Not currently implemented. Mentioned in CLAUDE.md architecture notes as a potential optimization pattern (use `get_dev_dataframe()` vs full dataset). Future consideration for memory-efficient processing of large datasets.

**Related Terms:** Feature Reduction

---

### Prebinning

**Definition:** The initial binning step that creates a large number of candidate bins (prebins) before merging them into final bins based on optimization criteria.

**Context in Lo-phi:** Controlled by `--prebins` parameter (default 20). Prebins are created using either quantile or CART strategy, then merged via greedy algorithm or MIP solver to reach target bin count (default 10). Defined as `DEFAULT_PREBINS` constant in `src/pipeline/iv.rs` line 19.

**Related Terms:** Binning, Quantile Binning, CART, Solver

---

### Parquet

**Definition:** A columnar storage file format designed for efficient data processing. Supports schema preservation, compression, and fast column-level reads without loading entire rows.

**Context in Lo-phi:** Supported as both input and output format alongside CSV. Parquet files preserve column data types, avoiding schema inference issues. The `convert` subcommand converts CSV to Parquet. See ADR-007 for the dual format design decision.

**Related Terms:** Schema Inference

---

### Quantile Binning

**Definition:** A binning strategy that creates bins with approximately equal sample counts by partitioning data at quantile boundaries.

**Context in Lo-phi:** Available as `BinningStrategy::Quantile`. Alternative to CART binning. Creates uniform-frequency bins which can be advantageous for skewed distributions. Configured via `--binning-strategy quantile` CLI flag.

**Related Terms:** Binning, CART, Prebinning

---

### Schema Inference

**Definition:** The process of automatically detecting column data types (numeric, string, boolean, etc.) by sampling rows from a dataset.

**Context in Lo-phi:** Configurable via interactive TUI under "Advanced options" (default 10000 rows scanned). Setting to 0 performs full scan. Balances accuracy vs. speed when loading CSV files. Impacts how Polars interprets column types during initial data load.

**Related Terms:** Feature Reduction

---

### Scorecard

**Definition:** A credit risk model that assigns points to each feature's WoE bin, producing a total score representing the probability of default. Standard methodology in consumer lending and financial services.

**Context in Lo-phi:** Lo-phi's WoE binning and IV analysis follow scorecard development conventions. The `ln(Bad/Good)` WoE sign convention (ADR-004) aligns with industry-standard scorecard development where positive WoE indicates higher risk.

**Related Terms:** Weight of Evidence, Information Value, Bad Rate

---

### Solver

**Definition:** An optimization engine (specifically, the HiGHS MIP solver) that finds globally optimal binning solutions by maximizing Information Value subject to constraints.

**Context in Lo-phi:** Implemented in `src/pipeline/solver/` module. Enabled by default with 30-second timeout and 0.01 gap tolerance. Can enforce monotonicity constraints. Falls back to greedy merging on timeout. Toggled via `--use-solver` flag in CLI or `[S]` key in TUI.

**Related Terms:** MIP, Gap Tolerance, Monotonicity Constraint, Prebinning

---

### Weighted Analysis

**Definition:** Statistical analysis where each observation has an associated weight, allowing unequal importance of data points. Weights scale contributions to summary statistics.

**Context in Lo-phi:** Fully supported throughout pipeline. Weight column selected via `--weight-column` CLI flag or `[W]` key in TUI. Affects null ratio, WoE/IV calculation, and Pearson correlation. When no weight column is specified, uniform weights of 1.0 are used.

**Related Terms:** Null Ratio, Weight of Evidence, Pearson Correlation

---

### Welford Algorithm

**Definition:** A numerically stable single-pass algorithm for computing variance and covariance. Avoids catastrophic cancellation errors common in naive two-pass methods.

**Context in Lo-phi:** Implemented in `src/pipeline/correlation.rs` for weighted Pearson correlation calculation (lines 166-286). Computes weighted mean, variance, and covariance simultaneously in one pass through the data, critical for large datasets.

**Related Terms:** Pearson Correlation, Weighted Analysis

**Formula:**
For each new observation, update running statistics:
$$M_{n} = M_{n-1} + \frac{w_n (x_n - M_{n-1})}{\sum_{i=1}^{n} w_i}$$
where $M_n$ is the weighted mean after $n$ observations.

---

### Weight of Evidence (WoE)

**Definition:** A measure of the predictive strength of a bin, calculated as the natural logarithm of the ratio of event distribution to non-event distribution. Positive WoE indicates higher risk (more events), negative WoE indicates lower risk.

**Context in Lo-phi:** Core calculation in `src/pipeline/iv.rs` (function `calculate_woe_iv` at line 1455). Uses `ln(%bad/%good)` convention with Laplace smoothing (SMOOTHING = 0.5). Stored in `WoeBin.woe` and `CategoricalWoeBin.woe`. Forms the basis for IV calculation.

**Related Terms:** Information Value, Laplace Smoothing, Bad Rate, Good Rate

**Formula:**
$$\text{WoE} = \ln\left(\frac{\%\text{events}}{\%\text{non\_events}}\right) = \ln\left(\frac{\frac{\text{events} + 0.5}{\text{total\_events} + 0.5}}{\frac{\text{non\_events} + 0.5}{\text{total\_non\_events} + 0.5}}\right)$$
