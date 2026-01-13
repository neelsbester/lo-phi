//! Benchmark comparing Quantile vs CART binning strategies and greedy vs solver optimization
//!
//! Run with: cargo bench --bench binning_benchmark

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use polars::prelude::*;
use rand::prelude::*;
use rand::SeedableRng;

use lophi::pipeline::{analyze_features_iv, BinningStrategy, MonotonicityConstraint, SolverConfig};

/// Generate synthetic data with controlled characteristics
fn generate_test_dataframe(n_rows: usize, n_features: usize, seed: u64) -> DataFrame {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    // Create target column (binary)
    let target: Vec<i32> = (0..n_rows)
        .map(|_| if rng.gen::<f64>() > 0.7 { 1 } else { 0 })
        .collect();

    // Create feature columns with varying characteristics
    let mut columns: Vec<Column> = vec![Column::new("target".into(), target.clone())];

    for i in 0..n_features {
        let feature_type = i % 3; // Cycle through different distributions

        let values: Vec<f64> = match feature_type {
            0 => {
                // Normal distribution - good for quantile binning
                (0..n_rows).map(|_| rng.gen::<f64>() * 100.0).collect()
            }
            1 => {
                // Skewed distribution - may favor CART
                (0..n_rows)
                    .map(|_| {
                        let v = rng.gen::<f64>();
                        (v * v * v) * 100.0 // Right-skewed
                    })
                    .collect()
            }
            _ => {
                // Feature correlated with target - should show clear splits
                (0..n_rows)
                    .enumerate()
                    .map(|(idx, _)| {
                        let base = if target[idx] == 1 { 70.0 } else { 30.0 };
                        base + rng.gen::<f64>() * 20.0 - 10.0
                    })
                    .collect()
            }
        };

        columns.push(Column::new(format!("feature_{}", i).into(), values));
    }

    DataFrame::new(columns).expect("Failed to create DataFrame")
}

/// Benchmark quantile vs CART binning for varying dataset sizes
fn benchmark_binning_strategies(c: &mut Criterion) {
    let mut group = c.benchmark_group("binning_strategies");

    // Test different dataset sizes
    let sizes = [(1_000, 10), (5_000, 20), (10_000, 50)];

    for (n_rows, n_features) in sizes {
        let df = generate_test_dataframe(n_rows, n_features, 42);
        let weights = vec![1.0; df.height()];
        group.throughput(Throughput::Elements(n_features as u64));

        group.bench_with_input(
            BenchmarkId::new("quantile", format!("{}x{}", n_rows, n_features)),
            &df,
            |b, df| {
                b.iter(|| {
                    let _ = analyze_features_iv(
                        black_box(df),
                        black_box("target"),
                        black_box(10),
                        black_box(20),
                        black_box(None),
                        black_box(BinningStrategy::Quantile),
                        black_box(None),
                        black_box(None),
                        black_box(&weights),
                        black_box(None),
                        black_box(None),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("cart", format!("{}x{}", n_rows, n_features)),
            &df,
            |b, df| {
                b.iter(|| {
                    let _ = analyze_features_iv(
                        black_box(df),
                        black_box("target"),
                        black_box(10),
                        black_box(20),
                        black_box(None),
                        black_box(BinningStrategy::Cart),
                        black_box(None),
                        black_box(None),
                        black_box(&weights),
                        black_box(None),
                        black_box(None),
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark focusing on a single large feature to isolate binning performance
fn benchmark_single_feature(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_feature_binning");

    let sizes = [10_000, 50_000, 100_000];

    for n_rows in sizes {
        // Single feature dataframe
        let df = generate_test_dataframe(n_rows, 1, 42);
        let weights = vec![1.0; df.height()];
        group.throughput(Throughput::Elements(n_rows as u64));

        group.bench_with_input(BenchmarkId::new("quantile", n_rows), &df, |b, df| {
            b.iter(|| {
                let _ = analyze_features_iv(
                    black_box(df),
                    black_box("target"),
                    black_box(10),
                    black_box(20),
                    black_box(None),
                    black_box(BinningStrategy::Quantile),
                    black_box(None),
                    black_box(None),
                    black_box(&weights),
                    black_box(None),
                    black_box(None),
                );
            });
        });

        group.bench_with_input(BenchmarkId::new("cart", n_rows), &df, |b, df| {
            b.iter(|| {
                let _ = analyze_features_iv(
                    black_box(df),
                    black_box("target"),
                    black_box(10),
                    black_box(20),
                    black_box(None),
                    black_box(BinningStrategy::Cart),
                    black_box(None),
                    black_box(None),
                    black_box(&weights),
                    black_box(None),
                    black_box(None),
                );
            });
        });
    }

    group.finish();
}

/// Benchmark with varying number of bins
fn benchmark_bin_counts(c: &mut Criterion) {
    let mut group = c.benchmark_group("bin_count_impact");

    let df = generate_test_dataframe(10_000, 10, 42);
    let weights = vec![1.0; df.height()];
    let bin_counts = [5, 10, 20, 50];

    for num_bins in bin_counts {
        group.bench_with_input(
            BenchmarkId::new("quantile", num_bins),
            &num_bins,
            |b, &num_bins| {
                b.iter(|| {
                    let _ = analyze_features_iv(
                        black_box(&df),
                        black_box("target"),
                        black_box(num_bins),
                        black_box(20),
                        black_box(None),
                        black_box(BinningStrategy::Quantile),
                        black_box(None),
                        black_box(None),
                        black_box(&weights),
                        black_box(None),
                        black_box(None),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("cart", num_bins),
            &num_bins,
            |b, &num_bins| {
                b.iter(|| {
                    let _ = analyze_features_iv(
                        black_box(&df),
                        black_box("target"),
                        black_box(num_bins),
                        black_box(20),
                        black_box(None),
                        black_box(BinningStrategy::Cart),
                        black_box(None),
                        black_box(None),
                        black_box(&weights),
                        black_box(None),
                        black_box(None),
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark comparing greedy merge vs MIP solver optimization
fn benchmark_greedy_vs_solver(c: &mut Criterion) {
    let mut group = c.benchmark_group("greedy_vs_solver");
    group.sample_size(20); // Fewer samples due to solver time

    // Smaller sizes for solver benchmarks
    let sizes = [(1_000, 5), (5_000, 10)];

    let solver_config = SolverConfig {
        timeout_seconds: 30,
        gap_tolerance: 0.01,
        monotonicity: MonotonicityConstraint::None,
        min_bin_samples: 5,
    };

    for (n_rows, n_features) in sizes {
        let df = generate_test_dataframe(n_rows, n_features, 42);
        let weights = vec![1.0; df.height()];
        group.throughput(Throughput::Elements(n_features as u64));

        // Greedy (no solver)
        group.bench_with_input(
            BenchmarkId::new("greedy", format!("{}x{}", n_rows, n_features)),
            &df,
            |b, df| {
                b.iter(|| {
                    let _ = analyze_features_iv(
                        black_box(df),
                        black_box("target"),
                        black_box(10),
                        black_box(20),
                        black_box(None),
                        black_box(BinningStrategy::Cart),
                        black_box(None),
                        black_box(None),
                        black_box(&weights),
                        black_box(None),
                        black_box(None), // No solver
                    );
                });
            },
        );

        // MIP Solver
        group.bench_with_input(
            BenchmarkId::new("solver", format!("{}x{}", n_rows, n_features)),
            &df,
            |b, df| {
                b.iter(|| {
                    let _ = analyze_features_iv(
                        black_box(df),
                        black_box("target"),
                        black_box(10),
                        black_box(20),
                        black_box(None),
                        black_box(BinningStrategy::Cart),
                        black_box(None),
                        black_box(None),
                        black_box(&weights),
                        black_box(None),
                        black_box(Some(&solver_config)),
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark impact of monotonicity constraints on solver performance
fn benchmark_monotonicity_impact(c: &mut Criterion) {
    let mut group = c.benchmark_group("monotonicity_impact");
    group.sample_size(15); // Fewer samples due to solver time

    let df = generate_test_dataframe(5_000, 5, 42);
    let weights = vec![1.0; df.height()];

    let monotonicity_variants = [
        ("none", MonotonicityConstraint::None),
        ("ascending", MonotonicityConstraint::Ascending),
        ("descending", MonotonicityConstraint::Descending),
        ("auto", MonotonicityConstraint::Auto),
    ];

    for (name, monotonicity) in monotonicity_variants {
        let config = SolverConfig {
            timeout_seconds: 30,
            gap_tolerance: 0.01,
            monotonicity,
            min_bin_samples: 5,
        };

        group.bench_with_input(BenchmarkId::new("solver", name), &config, |b, config| {
            b.iter(|| {
                let _ = analyze_features_iv(
                    black_box(&df),
                    black_box("target"),
                    black_box(10),
                    black_box(20),
                    black_box(None),
                    black_box(BinningStrategy::Cart),
                    black_box(None),
                    black_box(None),
                    black_box(&weights),
                    black_box(None),
                    black_box(Some(config)),
                );
            });
        });
    }

    group.finish();
}

/// Benchmark impact of prebins count on performance
fn benchmark_prebins_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("prebins_count");

    let df = generate_test_dataframe(10_000, 10, 42);
    let weights = vec![1.0; df.height()];
    let prebin_counts = [10, 20, 50, 100];

    for prebins in prebin_counts {
        group.bench_with_input(
            BenchmarkId::new("greedy", prebins),
            &prebins,
            |b, &prebins| {
                b.iter(|| {
                    let _ = analyze_features_iv(
                        black_box(&df),
                        black_box("target"),
                        black_box(10),
                        black_box(prebins),
                        black_box(None),
                        black_box(BinningStrategy::Cart),
                        black_box(None),
                        black_box(None),
                        black_box(&weights),
                        black_box(None),
                        black_box(None),
                    );
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_binning_strategies,
    benchmark_single_feature,
    benchmark_bin_counts,
    benchmark_greedy_vs_solver,
    benchmark_monotonicity_impact,
    benchmark_prebins_count,
);
criterion_main!(benches);
