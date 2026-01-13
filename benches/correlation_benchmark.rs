//! Benchmark comparing Pairwise vs Matrix-based correlation computation
//!
//! Run with: cargo bench --bench correlation_benchmark

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use polars::prelude::*;
use rand::prelude::*;
use rand::SeedableRng;

use lophi::pipeline::{find_correlated_pairs, find_correlated_pairs_matrix};

/// Generate synthetic data with controlled characteristics
fn generate_test_dataframe(n_rows: usize, n_features: usize, seed: u64) -> DataFrame {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    // Create feature columns with varying characteristics
    let mut columns: Vec<Column> = Vec::with_capacity(n_features);

    for i in 0..n_features {
        let feature_type = i % 4; // Cycle through different distributions

        let values: Vec<f64> = match feature_type {
            0 => {
                // Normal distribution
                (0..n_rows).map(|_| rng.gen::<f64>() * 100.0).collect()
            }
            1 => {
                // Skewed distribution
                (0..n_rows)
                    .map(|_| {
                        let v = rng.gen::<f64>();
                        (v * v * v) * 100.0
                    })
                    .collect()
            }
            2 => {
                // Bimodal distribution
                (0..n_rows)
                    .map(|_| {
                        if rng.gen::<bool>() {
                            rng.gen::<f64>() * 30.0
                        } else {
                            70.0 + rng.gen::<f64>() * 30.0
                        }
                    })
                    .collect()
            }
            _ => {
                // Correlated with another feature (creates correlation pairs)
                let base_idx = i.saturating_sub(3);
                if base_idx < columns.len() {
                    // Get values from an earlier column and add noise
                    columns[base_idx]
                        .f64()
                        .unwrap()
                        .into_iter()
                        .map(|v| v.unwrap_or(50.0) + rng.gen::<f64>() * 10.0 - 5.0)
                        .collect()
                } else {
                    (0..n_rows).map(|_| rng.gen::<f64>() * 100.0).collect()
                }
            }
        };

        columns.push(Column::new(format!("feature_{}", i).into(), values));
    }

    DataFrame::new(columns).expect("Failed to create DataFrame")
}

/// Benchmark pairwise vs matrix correlation for varying column counts
fn benchmark_correlation_by_columns(c: &mut Criterion) {
    let mut group = c.benchmark_group("correlation_by_columns");
    group.sample_size(30);

    // Fixed row count, varying column count
    let n_rows = 10_000;
    let column_counts = [10, 25, 50, 100, 200];

    for n_cols in column_counts {
        let df = generate_test_dataframe(n_rows, n_cols, 42);
        let weights = vec![1.0; df.height()];
        let threshold = 0.8;

        group.throughput(Throughput::Elements(((n_cols * (n_cols - 1)) / 2) as u64));

        group.bench_with_input(
            BenchmarkId::new("pairwise", n_cols),
            &(&df, &weights),
            |b, (df, weights)| {
                b.iter(|| {
                    let _ = find_correlated_pairs(
                        black_box(*df),
                        black_box(threshold),
                        black_box(*weights),
                        black_box(None),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("matrix", n_cols),
            &(&df, &weights),
            |b, (df, weights)| {
                b.iter(|| {
                    let _ = find_correlated_pairs_matrix(
                        black_box(*df),
                        black_box(threshold),
                        black_box(*weights),
                        black_box(None),
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark pairwise vs matrix correlation for varying row counts
fn benchmark_correlation_by_rows(c: &mut Criterion) {
    let mut group = c.benchmark_group("correlation_by_rows");
    group.sample_size(20);

    // Fixed column count, varying row count
    let n_cols = 50;
    let row_counts = [1_000, 5_000, 10_000, 50_000, 100_000];

    for n_rows in row_counts {
        let df = generate_test_dataframe(n_rows, n_cols, 42);
        let weights = vec![1.0; df.height()];
        let threshold = 0.8;

        group.throughput(Throughput::Elements(n_rows as u64));

        group.bench_with_input(
            BenchmarkId::new("pairwise", n_rows),
            &(&df, &weights),
            |b, (df, weights)| {
                b.iter(|| {
                    let _ = find_correlated_pairs(
                        black_box(*df),
                        black_box(threshold),
                        black_box(*weights),
                        black_box(None),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("matrix", n_rows),
            &(&df, &weights),
            |b, (df, weights)| {
                b.iter(|| {
                    let _ = find_correlated_pairs_matrix(
                        black_box(*df),
                        black_box(threshold),
                        black_box(*weights),
                        black_box(None),
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark weighted vs unweighted correlation
fn benchmark_weighted_correlation(c: &mut Criterion) {
    let mut group = c.benchmark_group("weighted_correlation");
    group.sample_size(30);

    let n_rows = 10_000;
    let n_cols = 50;
    let threshold = 0.8;

    let df = generate_test_dataframe(n_rows, n_cols, 42);

    // Uniform weights (equivalent to unweighted)
    let uniform_weights = vec![1.0; df.height()];

    // Non-uniform weights (some samples weighted higher)
    let mut rng = rand::rngs::StdRng::seed_from_u64(123);
    let varied_weights: Vec<f64> = (0..df.height())
        .map(|_| 0.5 + rng.gen::<f64>() * 1.5)
        .collect();

    group.bench_with_input(
        BenchmarkId::new("matrix", "uniform_weights"),
        &(&df, &uniform_weights),
        |b, (df, weights)| {
            b.iter(|| {
                let _ = find_correlated_pairs_matrix(
                    black_box(*df),
                    black_box(threshold),
                    black_box(*weights),
                    black_box(None),
                );
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("matrix", "varied_weights"),
        &(&df, &varied_weights),
        |b, (df, weights)| {
            b.iter(|| {
                let _ = find_correlated_pairs_matrix(
                    black_box(*df),
                    black_box(threshold),
                    black_box(*weights),
                    black_box(None),
                );
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("pairwise", "uniform_weights"),
        &(&df, &uniform_weights),
        |b, (df, weights)| {
            b.iter(|| {
                let _ = find_correlated_pairs(
                    black_box(*df),
                    black_box(threshold),
                    black_box(*weights),
                    black_box(None),
                );
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("pairwise", "varied_weights"),
        &(&df, &varied_weights),
        |b, (df, weights)| {
            b.iter(|| {
                let _ = find_correlated_pairs(
                    black_box(*df),
                    black_box(threshold),
                    black_box(*weights),
                    black_box(None),
                );
            });
        },
    );

    group.finish();
}

/// Large-scale benchmark for real-world scenario simulation
fn benchmark_large_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_scale_correlation");
    group.sample_size(10);

    // Simulate real-world credit scoring scenario: many rows, moderate columns
    let scenarios = [
        ("small_dataset", 5_000, 30),
        ("medium_dataset", 50_000, 50),
        ("large_dataset", 100_000, 100),
    ];

    for (name, n_rows, n_cols) in scenarios {
        let df = generate_test_dataframe(n_rows, n_cols, 42);
        let weights = vec![1.0; df.height()];
        let threshold = 0.8;

        group.throughput(Throughput::Elements(((n_cols * (n_cols - 1)) / 2) as u64));

        group.bench_with_input(
            BenchmarkId::new("pairwise", name),
            &(&df, &weights),
            |b, (df, weights)| {
                b.iter(|| {
                    let _ = find_correlated_pairs(
                        black_box(*df),
                        black_box(threshold),
                        black_box(*weights),
                        black_box(None),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("matrix", name),
            &(&df, &weights),
            |b, (df, weights)| {
                b.iter(|| {
                    let _ = find_correlated_pairs_matrix(
                        black_box(*df),
                        black_box(threshold),
                        black_box(*weights),
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
    benchmark_correlation_by_columns,
    benchmark_correlation_by_rows,
    benchmark_weighted_correlation,
    benchmark_large_scale,
);
criterion_main!(benches);
