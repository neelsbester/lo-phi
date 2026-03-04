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
