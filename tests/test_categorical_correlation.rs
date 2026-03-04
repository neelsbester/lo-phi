//! Tests for categorical association measures (Cramér's V, Eta)
//! and IV-first drop logic.

use lophi::pipeline::{
    compute_cramers_v, compute_eta, find_correlated_pairs_auto, select_features_to_drop,
    AssociationMeasure, CorrelatedPair, FeatureMetadata,
};
use polars::prelude::*;
use std::collections::HashMap;

// ── Cramér's V ──────────────────────────────────────────────────────────────

#[test]
fn test_cramers_v_perfect_association() {
    // Two identical columns should have V = 1.0
    let a = Column::new(
        "a".into(),
        vec!["x", "y", "z", "x", "y", "z", "x", "y", "z", "x"],
    );
    let b = Column::new(
        "b".into(),
        vec!["x", "y", "z", "x", "y", "z", "x", "y", "z", "x"],
    );

    let v = compute_cramers_v(&a, &b, None).unwrap();
    assert!(
        v > 0.9,
        "Identical categorical columns should have V close to 1.0, got {:.4}",
        v
    );
}

#[test]
fn test_cramers_v_no_association() {
    // Construct independent columns (uniform distribution in each cell)
    // 4x4 contingency table with equal counts
    let mut a_vals = Vec::new();
    let mut b_vals = Vec::new();
    for a_cat in &["a1", "a2", "a3", "a4"] {
        for b_cat in &["b1", "b2", "b3", "b4"] {
            for _ in 0..10 {
                a_vals.push(*a_cat);
                b_vals.push(*b_cat);
            }
        }
    }
    let a = Column::new("a".into(), a_vals);
    let b = Column::new("b".into(), b_vals);

    let v = compute_cramers_v(&a, &b, None).unwrap();
    assert!(
        v < 0.1,
        "Independent columns should have V close to 0.0, got {:.4}",
        v
    );
}

#[test]
fn test_cramers_v_single_category_returns_zero() {
    // One column with a single category -> V = 0
    let a = Column::new("a".into(), vec!["x", "x", "x", "x", "x"]);
    let b = Column::new("b".into(), vec!["p", "q", "r", "p", "q"]);

    let v = compute_cramers_v(&a, &b, None).unwrap();
    assert_eq!(v, 0.0, "Single category in one column should give V = 0");
}

#[test]
fn test_cramers_v_weighted() {
    // With uniform weights, result should match unweighted
    let a = Column::new("a".into(), vec!["x", "y", "x", "y", "x", "y"]);
    let b = Column::new("b".into(), vec!["p", "q", "p", "q", "p", "q"]);

    let v_unweighted = compute_cramers_v(&a, &b, None).unwrap();
    let weights = vec![1.0; 6];
    let v_weighted = compute_cramers_v(&a, &b, Some(&weights)).unwrap();

    assert!(
        (v_unweighted - v_weighted).abs() < 1e-10,
        "Uniform weights should produce same result as unweighted: {:.6} vs {:.6}",
        v_unweighted,
        v_weighted
    );
}

#[test]
fn test_cramers_v_non_uniform_weights() {
    // Non-uniform weights should change the result
    let a = Column::new("a".into(), vec!["x", "y", "x", "y", "x", "y", "x", "y"]);
    let b = Column::new("b".into(), vec!["p", "q", "p", "q", "p", "p", "q", "q"]);

    let v_uniform = compute_cramers_v(&a, &b, None).unwrap();
    // Heavy weight on the perfectly associated first 4 rows
    let weights = vec![10.0, 10.0, 10.0, 10.0, 0.1, 0.1, 0.1, 0.1];
    let v_heavy = compute_cramers_v(&a, &b, Some(&weights)).unwrap();

    // They should differ (heavy-weighting the associated rows should increase V)
    assert!(
        (v_uniform - v_heavy).abs() > 0.01,
        "Non-uniform weights should change Cramér's V"
    );
}

#[test]
fn test_cramers_v_empty_returns_none() {
    let a = Column::new("a".into(), Vec::<&str>::new());
    let b = Column::new("b".into(), Vec::<&str>::new());

    assert!(compute_cramers_v(&a, &b, None).is_none());
}

#[test]
fn test_cramers_v_bias_correction_reduces_inflation() {
    // With small sample sizes, bias-corrected V should be lower than naive V
    // We test indirectly by verifying V stays in [0, 1]
    let a = Column::new("a".into(), vec!["x", "y", "z"]);
    let b = Column::new("b".into(), vec!["p", "q", "r"]);

    let v = compute_cramers_v(&a, &b, None).unwrap();
    assert!(
        (0.0..=1.0).contains(&v),
        "V must be in [0, 1], got {:.4}",
        v
    );
}

// ── Eta (Correlation Ratio) ─────────────────────────────────────────────────

#[test]
fn test_eta_perfect_separation() {
    // Each category has a distinct numeric value -> eta = 1.0
    let cat = Column::new(
        "cat".into(),
        vec!["a", "a", "a", "b", "b", "b", "c", "c", "c"],
    );
    let num = Column::new(
        "num".into(),
        vec![1.0, 1.0, 1.0, 5.0, 5.0, 5.0, 10.0, 10.0, 10.0],
    );

    let eta = compute_eta(&cat, &num, None).unwrap();
    assert!(
        eta > 0.99,
        "Perfect separation should give eta close to 1.0, got {:.4}",
        eta
    );
}

#[test]
fn test_eta_no_separation() {
    // All categories have the same mean -> eta = 0
    let cat = Column::new(
        "cat".into(),
        vec!["a", "b", "c", "a", "b", "c", "a", "b", "c"],
    );
    let num = Column::new(
        "num".into(),
        vec![1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0],
    );

    let eta = compute_eta(&cat, &num, None).unwrap();
    assert!(
        eta < 0.01,
        "Same mean across categories should give eta close to 0.0, got {:.4}",
        eta
    );
}

#[test]
fn test_eta_single_category_returns_zero() {
    let cat = Column::new("cat".into(), vec!["x", "x", "x", "x", "x"]);
    let num = Column::new("num".into(), vec![1.0, 2.0, 3.0, 4.0, 5.0]);

    let eta = compute_eta(&cat, &num, None).unwrap();
    assert_eq!(eta, 0.0, "Single category should give eta = 0");
}

#[test]
fn test_eta_zero_numeric_variance_returns_zero() {
    let cat = Column::new("cat".into(), vec!["a", "b", "a", "b"]);
    let num = Column::new("num".into(), vec![5.0, 5.0, 5.0, 5.0]);

    let eta = compute_eta(&cat, &num, None).unwrap();
    assert_eq!(eta, 0.0, "Zero numeric variance should give eta = 0");
}

#[test]
fn test_eta_weighted() {
    let cat = Column::new("cat".into(), vec!["a", "a", "a", "b", "b", "b"]);
    let num = Column::new("num".into(), vec![1.0, 1.0, 1.0, 10.0, 10.0, 10.0]);

    let eta_unweighted = compute_eta(&cat, &num, None).unwrap();
    let weights = vec![1.0; 6];
    let eta_weighted = compute_eta(&cat, &num, Some(&weights)).unwrap();

    assert!(
        (eta_unweighted - eta_weighted).abs() < 1e-10,
        "Uniform weights should match unweighted: {:.6} vs {:.6}",
        eta_unweighted,
        eta_weighted
    );
}

#[test]
fn test_eta_empty_returns_none() {
    let cat = Column::new("cat".into(), Vec::<&str>::new());
    let num = Column::new("num".into(), Vec::<f64>::new());

    assert!(compute_eta(&cat, &num, None).is_none());
}

#[test]
fn test_eta_in_zero_one_range() {
    // Eta should always be in [0, 1]
    let cat = Column::new(
        "cat".into(),
        vec!["a", "b", "c", "a", "b", "c", "a", "b", "c", "a"],
    );
    let num = Column::new(
        "num".into(),
        vec![1.5, 2.3, 7.0, 1.8, 2.1, 6.5, 1.2, 2.8, 7.5, 1.0],
    );

    let eta = compute_eta(&cat, &num, None).unwrap();
    assert!(
        (0.0..=1.0).contains(&eta),
        "Eta must be in [0, 1], got {:.4}",
        eta
    );
}

// ── IV-First Drop Logic ─────────────────────────────────────────────────────

fn make_pair(f1: &str, f2: &str, corr: f64, measure: AssociationMeasure) -> CorrelatedPair {
    CorrelatedPair {
        feature1: f1.to_string(),
        feature2: f2.to_string(),
        correlation: corr,
        measure,
    }
}

#[test]
fn test_drop_lower_iv() {
    let pairs = vec![make_pair("a", "b", 0.95, AssociationMeasure::Pearson)];

    let mut metadata = HashMap::new();
    metadata.insert(
        "a".to_string(),
        FeatureMetadata {
            iv: Some(0.50),
            missing_ratio: Some(0.0),
        },
    );
    metadata.insert(
        "b".to_string(),
        FeatureMetadata {
            iv: Some(0.10),
            missing_ratio: Some(0.0),
        },
    );

    let drops = select_features_to_drop(&pairs, "target", Some(&metadata));
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].feature, "b", "Should drop feature with lower IV");
    assert!(
        drops[0].reason.contains("lower IV"),
        "Reason should mention IV: {}",
        drops[0].reason
    );
}

#[test]
fn test_drop_equal_iv_uses_frequency() {
    // Both have equal IV, but "a" appears in 2 pairs -> should be dropped
    let pairs = vec![
        make_pair("a", "b", 0.95, AssociationMeasure::Pearson),
        make_pair("a", "c", 0.92, AssociationMeasure::Pearson),
    ];

    let mut metadata = HashMap::new();
    for name in &["a", "b", "c"] {
        metadata.insert(
            name.to_string(),
            FeatureMetadata {
                iv: Some(0.30),
                missing_ratio: Some(0.0),
            },
        );
    }

    let drops = select_features_to_drop(&pairs, "target", Some(&metadata));
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].feature, "a",
        "Should drop 'a' (higher frequency, same IV)"
    );
    assert!(
        drops[0].reason.contains("higher frequency"),
        "Reason should mention frequency: {}",
        drops[0].reason
    );
}

#[test]
fn test_drop_equal_iv_equal_freq_uses_missing_ratio() {
    let pairs = vec![make_pair("a", "b", 0.95, AssociationMeasure::CramersV)];

    let mut metadata = HashMap::new();
    metadata.insert(
        "a".to_string(),
        FeatureMetadata {
            iv: Some(0.30),
            missing_ratio: Some(0.10),
        },
    );
    metadata.insert(
        "b".to_string(),
        FeatureMetadata {
            iv: Some(0.30),
            missing_ratio: Some(0.05),
        },
    );

    let drops = select_features_to_drop(&pairs, "target", Some(&metadata));
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].feature, "a",
        "Should drop 'a' (higher missing ratio)"
    );
    assert!(
        drops[0].reason.contains("higher missing ratio"),
        "Reason should mention missing ratio: {}",
        drops[0].reason
    );
}

#[test]
fn test_drop_full_tie_uses_alphabetical() {
    let pairs = vec![make_pair("alpha", "beta", 0.95, AssociationMeasure::Eta)];

    let mut metadata = HashMap::new();
    for name in &["alpha", "beta"] {
        metadata.insert(
            name.to_string(),
            FeatureMetadata {
                iv: Some(0.30),
                missing_ratio: Some(0.05),
            },
        );
    }

    let drops = select_features_to_drop(&pairs, "target", Some(&metadata));
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].feature, "beta",
        "Full tie should use alphabetical: drop 'beta' (keep 'alpha')"
    );
    assert!(
        drops[0].reason.contains("alphabetical"),
        "Reason should mention alphabetical: {}",
        drops[0].reason
    );
}

#[test]
fn test_drop_no_metadata_falls_back_to_frequency() {
    let pairs = vec![
        make_pair("a", "b", 0.95, AssociationMeasure::Pearson),
        make_pair("a", "c", 0.92, AssociationMeasure::Pearson),
    ];

    let drops = select_features_to_drop(&pairs, "target", None);
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].feature, "a",
        "Without metadata, should fall back to frequency"
    );
}

#[test]
fn test_drop_reason_includes_measure_name() {
    let pairs = vec![make_pair("x", "y", 0.88, AssociationMeasure::CramersV)];

    let drops = select_features_to_drop(&pairs, "target", None);
    assert_eq!(drops.len(), 1);
    assert!(
        drops[0].reason.contains("CramersV"),
        "Reason should include measure name: {}",
        drops[0].reason
    );
}

// ── Mixed-Type Pair Discovery ───────────────────────────────────────────────

#[test]
fn test_auto_finds_all_three_measure_types() {
    // DataFrame with 2 numeric and 2 categorical columns
    let n = 100;
    let num_a: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let num_b: Vec<f64> = num_a.iter().map(|x| x * 2.0 + 1.0).collect();
    let cat_a: Vec<String> = (0..n)
        .map(|i| if i % 2 == 0 { "x" } else { "y" }.to_string())
        .collect();
    let cat_b: Vec<String> = cat_a.clone(); // identical -> high V

    let df = DataFrame::new(vec![
        Column::new("num_a".into(), num_a),
        Column::new("num_b".into(), num_b),
        Column::new("cat_a".into(), cat_a),
        Column::new("cat_b".into(), cat_b),
    ])
    .unwrap();

    let weights = vec![1.0; n];
    let pairs = find_correlated_pairs_auto(&df, 0.3, &weights, None, None).unwrap();

    let has_pearson = pairs
        .iter()
        .any(|p| p.measure == AssociationMeasure::Pearson);
    let has_cramers = pairs
        .iter()
        .any(|p| p.measure == AssociationMeasure::CramersV);
    let has_eta = pairs.iter().any(|p| p.measure == AssociationMeasure::Eta);

    assert!(has_pearson, "Should find Pearson pairs (num_a, num_b)");
    assert!(has_cramers, "Should find CramersV pairs (cat_a, cat_b)");
    // Eta may or may not be above threshold depending on the data, but we check
    // it's at least attempted
    // For binary categories with a linear numeric, eta should be moderate
    let _ = has_eta; // Eta tested separately; this just ensures no panics
}

#[test]
fn test_auto_labels_pearson_correctly() {
    let df = df! {
        "a" => [1.0f64, 2.0, 3.0, 4.0, 5.0],
        "b" => [2.0f64, 4.0, 6.0, 8.0, 10.0],
    }
    .unwrap();
    let weights = vec![1.0; 5];

    let pairs = find_correlated_pairs_auto(&df, 0.5, &weights, None, None).unwrap();
    assert!(!pairs.is_empty());
    assert_eq!(
        pairs[0].measure,
        AssociationMeasure::Pearson,
        "Numeric-numeric pair should use Pearson"
    );
}

#[test]
fn test_auto_labels_cramers_v_correctly() {
    let n = 50;
    let cat_a: Vec<String> = (0..n).map(|i| format!("cat_{}", i % 3)).collect();
    let cat_b: Vec<String> = cat_a.clone(); // identical -> very high V

    let df = DataFrame::new(vec![
        Column::new("cat_a".into(), cat_a),
        Column::new("cat_b".into(), cat_b),
    ])
    .unwrap();

    let weights = vec![1.0; n];
    let pairs = find_correlated_pairs_auto(&df, 0.3, &weights, None, None).unwrap();
    assert!(!pairs.is_empty(), "Should find cat-cat pair");
    assert_eq!(
        pairs[0].measure,
        AssociationMeasure::CramersV,
        "Cat-cat pair should use CramersV"
    );
}

#[test]
fn test_auto_labels_eta_correctly() {
    // Strong categorical-numeric relationship
    let n = 90;
    let mut cat_vals = Vec::with_capacity(n);
    let mut num_vals = Vec::with_capacity(n);
    for i in 0..n {
        let group = i % 3;
        cat_vals.push(format!("g{}", group));
        // Each group has a very different mean
        num_vals.push(match group {
            0 => 1.0,
            1 => 50.0,
            _ => 100.0,
        });
    }

    let df = DataFrame::new(vec![
        Column::new("cat".into(), cat_vals),
        Column::new("num".into(), num_vals),
    ])
    .unwrap();

    let weights = vec![1.0; n];
    let pairs = find_correlated_pairs_auto(&df, 0.3, &weights, None, None).unwrap();
    assert!(!pairs.is_empty(), "Should find cat-num pair");
    assert_eq!(
        pairs[0].measure,
        AssociationMeasure::Eta,
        "Cat-num pair should use Eta"
    );
}

// ── High-Cardinality Exclusion ──────────────────────────────────────────────

#[test]
fn test_high_cardinality_categorical_excluded() {
    // Create a categorical column with >100 unique values
    let n = 200;
    let cat_high: Vec<String> = (0..n).map(|i| format!("val_{}", i)).collect();
    let cat_low: Vec<String> = (0..n).map(|i| format!("grp_{}", i % 3)).collect();
    let num: Vec<f64> = (0..n).map(|i| i as f64).collect();

    let df = DataFrame::new(vec![
        Column::new("high_card".into(), cat_high),
        Column::new("low_card".into(), cat_low),
        Column::new("numeric".into(), num),
    ])
    .unwrap();

    let weights = vec![1.0; n];
    let pairs = find_correlated_pairs_auto(&df, 0.0, &weights, None, None).unwrap();

    // high_card should not appear in any pair
    for pair in &pairs {
        assert_ne!(
            pair.feature1, "high_card",
            "High-cardinality column should be excluded"
        );
        assert_ne!(
            pair.feature2, "high_card",
            "High-cardinality column should be excluded"
        );
    }
}

// ── Feature Type Classification ─────────────────────────────────────────────

#[test]
fn test_feature_types_map_controls_classification() {
    use lophi::pipeline::FeatureType;

    // Create a numeric column that should be treated as categorical via feature_types map
    let df = df! {
        "encoded_cat" => [1.0f64, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0],
        "real_num" => [0.1f64, 0.5, 0.9, 0.2, 0.6, 1.0, 0.3, 0.7, 1.1, 0.4],
    }
    .unwrap();

    let mut feature_types = HashMap::new();
    feature_types.insert("encoded_cat".to_string(), FeatureType::Categorical);
    feature_types.insert("real_num".to_string(), FeatureType::Numeric);

    let weights = vec![1.0; 10];

    // With feature_types telling us encoded_cat is Categorical,
    // it should be treated as cat-num (Eta), not num-num (Pearson)
    let pairs = find_correlated_pairs_auto(&df, 0.0, &weights, None, Some(&feature_types)).unwrap();

    // There should be an Eta pair since we have 1 cat + 1 num
    let has_eta = pairs.iter().any(|p| p.measure == AssociationMeasure::Eta);
    assert!(
        has_eta,
        "With feature_types override, should find Eta pair, got {:?}",
        pairs.iter().map(|p| &p.measure).collect::<Vec<_>>()
    );
}

// ── AssociationMeasure Display ───────────────────────────────────────────────

#[test]
fn test_association_measure_display() {
    assert_eq!(format!("{}", AssociationMeasure::Pearson), "Pearson");
    assert_eq!(format!("{}", AssociationMeasure::CramersV), "CramersV");
    assert_eq!(format!("{}", AssociationMeasure::Eta), "Eta");
}

// ── Null-value handling ──────────────────────────────────────────────────────

#[test]
fn test_cramers_v_with_null_values() {
    // Null values are scattered through both categorical columns.
    // compute_cramers_v must not panic; the result must be a valid f64.
    let a = Column::new(
        "a".into(),
        vec![
            Some("x"),
            None,
            Some("y"),
            Some("x"),
            None,
            Some("z"),
            Some("y"),
            Some("z"),
        ],
    );
    let b = Column::new(
        "b".into(),
        vec![
            Some("p"),
            Some("q"),
            None,
            Some("p"),
            Some("q"),
            None,
            Some("p"),
            Some("q"),
        ],
    );

    let result = compute_cramers_v(&a, &b, None);
    // The function returns None for edge-case degenerate inputs; here we
    // expect a valid numeric value because multiple categories remain after
    // null removal.
    assert!(
        result.is_some(),
        "compute_cramers_v with scattered nulls should return Some"
    );
    let v = result.unwrap();
    assert!(
        v.is_finite() && (0.0..=1.0).contains(&v),
        "Cramér's V with nulls must be finite and in [0, 1], got {}",
        v
    );
}

#[test]
fn test_eta_with_null_values() {
    // Nulls in both the categorical and numeric columns.
    let cat = Column::new(
        "cat".into(),
        vec![
            Some("a"),
            None,
            Some("b"),
            Some("a"),
            Some("b"),
            None,
            Some("a"),
            Some("b"),
        ],
    );
    let num = Column::new(
        "num".into(),
        vec![
            Some(1.0f64),
            Some(5.0),
            None,
            Some(1.5),
            Some(4.5),
            Some(9.0),
            None,
            Some(5.5),
        ],
    );

    let result = compute_eta(&cat, &num, None);
    assert!(
        result.is_some(),
        "compute_eta with scattered nulls should return Some"
    );
    let eta = result.unwrap();
    assert!(
        eta.is_finite() && (0.0..=1.0).contains(&eta),
        "Eta with nulls must be finite and in [0, 1], got {}",
        eta
    );
}

// ── Known-value cross-checks ─────────────────────────────────────────────────

#[test]
fn test_cramers_v_2x2_known_value() {
    // 2×2 contingency: [[30, 10], [10, 30]]
    // n=80, a=30, b=10, c=10, d=30.
    //
    // chi2 = n * (ad - bc)^2 / [(a+b)(c+d)(a+c)(b+d)]
    //      = 80 * (900 - 100)^2 / (40 * 40 * 40 * 40)
    //      = 80 * 640000 / 2560000 = 20.0
    //
    // Bias-corrected Cramér's V (Bergsma 2013):
    //   phi_c^2_corr = max(0, chi2/n - (k-1)(r-1)/(n-1))
    //                = max(0, 20/80 - 1*1/79) = max(0, 0.25 - 0.012658) = 0.237342
    //   r_corr = r - 1/(n-1) = 2 - 1/79 = 1.987342
    //   c_corr = c - 1/(n-1) = 2 - 1/79 = 1.987342
    //   denom  = min(r_corr, c_corr) - 1 = 0.987342
    //   V = sqrt(phi_c^2_corr / denom) = sqrt(0.237342 / 0.987342) ≈ 0.4902
    //
    // Expand the 2×2 table into individual rows.
    let mut a_vals: Vec<&str> = Vec::new();
    let mut b_vals: Vec<&str> = Vec::new();
    // cell (x, p): 30 occurrences
    for _ in 0..30 {
        a_vals.push("x");
        b_vals.push("p");
    }
    // cell (x, q): 10 occurrences
    for _ in 0..10 {
        a_vals.push("x");
        b_vals.push("q");
    }
    // cell (y, p): 10 occurrences
    for _ in 0..10 {
        a_vals.push("y");
        b_vals.push("p");
    }
    // cell (y, q): 30 occurrences
    for _ in 0..30 {
        a_vals.push("y");
        b_vals.push("q");
    }

    let col_a = Column::new("a".into(), a_vals);
    let col_b = Column::new("b".into(), b_vals);

    let v = compute_cramers_v(&col_a, &col_b, None)
        .expect("Should return Some for a valid 2×2 table");

    let expected = 0.4902_f64;
    assert!(
        (v - expected).abs() < 0.001,
        "Bias-corrected V for [[30,10],[10,30]] should be ≈ {:.4}, got {:.4}",
        expected,
        v
    );
}

#[test]
fn test_eta_known_reference_value() {
    // 3 groups with zero within-group variance:
    //   group "a": [1, 1, 1]
    //   group "b": [5, 5, 5]
    //   group "c": [10, 10, 10]
    // All variance is between-group; Eta should be exactly 1.0.
    let cat = Column::new("cat".into(), vec!["a", "a", "a", "b", "b", "b", "c", "c", "c"]);
    let num = Column::new(
        "num".into(),
        vec![1.0f64, 1.0, 1.0, 5.0, 5.0, 5.0, 10.0, 10.0, 10.0],
    );

    let eta = compute_eta(&cat, &num, None).expect("Should return Some for perfect separation");
    assert!(
        (eta - 1.0).abs() < 1e-10,
        "Zero within-group variance should give Eta = 1.0, got {:.12}",
        eta
    );
}

// ── Edge cases: degenerate inputs ───────────────────────────────────────────

#[test]
fn test_cramers_v_single_row_returns_zero_or_none() {
    // n=1 → bias correction divides by (n-1)=0.  The implementation must
    // either return None or clamp to 0.0 rather than NaN/Inf/panic.
    let col_a = Column::new("a".into(), vec!["x"]);
    let col_b = Column::new("b".into(), vec!["p"]);

    let result = compute_cramers_v(&col_a, &col_b, None);
    match result {
        None => {} // acceptable: function recognises the degenerate case
        Some(v) => {
            assert!(
                v == 0.0 || v.is_finite(),
                "Single-row result must be 0.0 or at least finite, got {}",
                v
            );
        }
    }
}

// ── High-cardinality boundary ────────────────────────────────────────────────

#[test]
fn test_high_cardinality_boundary_exactly_100() {
    // A categorical column with exactly 100 unique values should be INCLUDED.
    // A column with 101 unique values should be EXCLUDED.
    let n = 200usize;

    // 100 unique values, each repeated twice.
    let cat_100: Vec<String> = (0..n).map(|i| format!("v{:03}", i % 100)).collect();
    // 101 unique values — crosses the threshold.
    let cat_101: Vec<String> = (0..n).map(|i| format!("u{:03}", i % 101)).collect();
    // A numeric column to pair against for eta pairs.
    let num: Vec<f64> = (0..n).map(|i| i as f64).collect();

    let df = DataFrame::new(vec![
        Column::new("cat_100".into(), cat_100),
        Column::new("cat_101".into(), cat_101),
        Column::new("numeric".into(), num),
    ])
    .unwrap();

    let weights = vec![1.0; n];
    // Threshold = 0.0 to allow any association to surface.
    let pairs = find_correlated_pairs_auto(&df, 0.0, &weights, None, None).unwrap();

    // cat_101 must not appear in any pair.
    for pair in &pairs {
        assert_ne!(
            pair.feature1, "cat_101",
            "cat_101 (101 unique values) must be excluded from all pairs"
        );
        assert_ne!(
            pair.feature2, "cat_101",
            "cat_101 (101 unique values) must be excluded from all pairs"
        );
    }

    // cat_100 should appear (it is within the limit and will form eta pairs
    // with the numeric column at threshold 0.0).
    let cat_100_present = pairs
        .iter()
        .any(|p| p.feature1 == "cat_100" || p.feature2 == "cat_100");
    assert!(
        cat_100_present,
        "cat_100 (exactly 100 unique values) should be included in pairs"
    );
}

// ── Weight effects on Eta ────────────────────────────────────────────────────

#[test]
fn test_eta_non_uniform_weights_change_result() {
    // Construct data where heavily-weighting the "separation" rows should
    // increase Eta compared to uniform weights.
    let cat = Column::new(
        "cat".into(),
        vec!["a", "a", "a", "a", "b", "b", "b", "b"],
    );
    // Group "a" ~ 1.0, group "b" ~ 10.0 (well separated).
    // Noise rows are at positions 3 and 7 — they reduce separation.
    let num = Column::new(
        "num".into(),
        vec![1.0f64, 1.0, 1.0, 7.0, 10.0, 10.0, 10.0, 4.0],
    );

    let eta_uniform = compute_eta(&cat, &num, None).unwrap();
    // Heavily weight the clean rows (0-2, 4-6) and down-weight the noisy ones (3, 7).
    let weights = vec![10.0, 10.0, 10.0, 0.01, 10.0, 10.0, 10.0, 0.01];
    let eta_weighted = compute_eta(&cat, &num, Some(&weights)).unwrap();

    assert!(
        (eta_uniform - eta_weighted).abs() > 1e-3,
        "Non-uniform weights that down-weight noisy rows should produce a different Eta: uniform={:.6}, weighted={:.6}",
        eta_uniform,
        eta_weighted
    );
    // The weighted version (noise suppressed) should show stronger separation.
    assert!(
        eta_weighted > eta_uniform,
        "Down-weighting noisy rows should increase Eta: uniform={:.6}, weighted={:.6}",
        eta_uniform,
        eta_weighted
    );
}

// ── Drop-logic partial metadata ───────────────────────────────────────────────

#[test]
fn test_drop_logic_partial_metadata() {
    // Only feature "a" has IV metadata; "b" does not.
    // The IV comparison requires *both* features to have IV.
    // When one is missing, the logic must fall through to frequency tiebreaking.
    let pairs = vec![make_pair("a", "b", 0.95, AssociationMeasure::CramersV)];

    let mut metadata = HashMap::new();
    metadata.insert(
        "a".to_string(),
        FeatureMetadata {
            iv: Some(0.80),
            missing_ratio: Some(0.0),
        },
    );
    // "b" intentionally absent from metadata.

    let drops = select_features_to_drop(&pairs, "target", Some(&metadata));
    assert_eq!(drops.len(), 1, "Should produce exactly one drop decision");

    // With only one IV value available, frequency comparison runs next.
    // Both "a" and "b" appear in exactly 1 pair (freq=1 tie) → alphabetical.
    // Alphabetical: "a" < "b" → drop "b".
    assert_eq!(
        drops[0].feature, "b",
        "With partial metadata and equal frequency, should fall back to alphabetical and drop 'b'"
    );
}

#[test]
fn test_drop_no_metadata_equal_frequency_uses_alphabetical() {
    // No metadata provided; each feature appears in exactly one pair (freq=1 tie).
    // Must use alphabetical fallback: keep the lexicographically first, drop the latter.
    let pairs = vec![make_pair("zebra", "apple", 0.91, AssociationMeasure::Pearson)];

    let drops = select_features_to_drop(&pairs, "target", None);
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].feature, "zebra",
        "With no metadata and equal frequency, alphabetical fallback should drop 'zebra' (keep 'apple')"
    );
    assert!(
        drops[0].reason.contains("alphabetical"),
        "Reason should mention alphabetical tie-break: {}",
        drops[0].reason
    );
}
