#!/usr/bin/env python3
"""Generate large synthetic dataset for performance testing (CSV + Parquet)."""

import numpy as np
import pandas as pd
import argparse
from pathlib import Path


def generate_dataset(
    rows: int = 100_000,
    num_cols: int = 4500,
    cat_cols: int = 400,
    missing_rate: float = 0.15,
    correlated_pairs: int = 100,
    high_missing_cols: int = 50,
    output_dir: str = "test_data",
    base_name: str = "large_test",
):
    """Generate a large dataset with controlled characteristics.
    
    The generated dataset includes:
    - Numeric columns with random values
    - Correlated column pairs (for testing correlation reduction)
    - Categorical columns
    - Columns with high missing rates (should be dropped by missing threshold)
    - A binary target column
    """
    total_cols = num_cols + correlated_pairs + cat_cols + high_missing_cols + 1
    print(f"Generating {rows:,} rows x {total_cols:,} columns...")

    # Set seed for reproducibility
    np.random.seed(42)

    data = {}

    # Generate numeric columns
    print(f"  Creating {num_cols} numeric columns...")
    for i in range(num_cols):
        data[f"num_{i:04d}"] = np.random.randn(rows).astype(np.float32)

    # Add correlated pairs (for testing correlation reduction)
    print(f"  Creating {correlated_pairs} correlated column pairs...")
    for i in range(correlated_pairs):
        base_col = f"num_{i:04d}"
        noise = np.random.randn(rows) * 0.05
        data[f"num_corr_{i:04d}"] = (data[base_col] + noise).astype(np.float32)

    # Generate categorical columns
    print(f"  Creating {cat_cols} categorical columns...")
    categories = ["A", "B", "C", "D", "E", None]
    for i in range(cat_cols):
        data[f"cat_{i:04d}"] = np.random.choice(categories, rows)

    # Add target column
    data["target"] = np.random.randint(0, 2, rows)

    print("  Building DataFrame...")
    df = pd.DataFrame(data)

    # Introduce missing values in numeric columns
    print("  Introducing missing values...")
    numeric_cols_list = [c for c in df.columns if c.startswith("num_") and not c.startswith("num_corr_")]
    for col in numeric_cols_list:
        mask = np.random.random(rows) < missing_rate
        df.loc[mask, col] = np.nan

    # Some columns with very high missing rate (should be dropped)
    print(f"  Creating {high_missing_cols} high-missing columns...")
    for i in range(high_missing_cols):
        high_missing_col = f"high_missing_{i:04d}"
        df[high_missing_col] = np.random.randn(rows).astype(np.float32)
        mask = np.random.random(rows) < 0.5  # 50% missing
        df.loc[mask, high_missing_col] = np.nan

    # Create output directory
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)

    parquet_file = output_path / f"{base_name}.parquet"
    csv_file = output_path / f"{base_name}.csv"

    # Write Parquet (faster, smaller)
    print(f"Writing {parquet_file}...")
    df.to_parquet(parquet_file, index=False, compression="snappy")
    parquet_size = parquet_file.stat().st_size / (1024**3)
    print(f"  Parquet size: {parquet_size:.2f} GB")

    # Write CSV (larger, slower but universal)
    print(f"Writing {csv_file}...")
    df.to_csv(csv_file, index=False)
    csv_size = csv_file.stat().st_size / (1024**3)
    print(f"  CSV size: {csv_size:.2f} GB")

    print(f"\nDone!")
    print(f"Shape: {df.shape}")
    print(f"Files created:")
    print(f"  - {parquet_file} ({parquet_size:.2f} GB)")
    print(f"  - {csv_file} ({csv_size:.2f} GB)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate test data for lophi benchmarks"
    )
    parser.add_argument(
        "--rows",
        type=int,
        default=100_000,
        help="Number of rows (default: 100,000)",
    )
    parser.add_argument(
        "--num-cols",
        type=int,
        default=4500,
        help="Number of numeric columns (default: 4500)",
    )
    parser.add_argument(
        "--cat-cols",
        type=int,
        default=400,
        help="Number of categorical columns (default: 400)",
    )
    parser.add_argument(
        "--correlated-pairs",
        type=int,
        default=100,
        help="Number of correlated column pairs (default: 100)",
    )
    parser.add_argument(
        "--high-missing-cols",
        type=int,
        default=50,
        help="Number of high-missing columns (default: 50)",
    )
    parser.add_argument(
        "--output-dir",
        type=str,
        default="test_data",
        help="Output directory (default: test_data)",
    )
    parser.add_argument(
        "--base-name",
        type=str,
        default="large_test",
        help="Base filename without extension (default: large_test)",
    )
    args = parser.parse_args()

    generate_dataset(
        rows=args.rows,
        num_cols=args.num_cols,
        cat_cols=args.cat_cols,
        correlated_pairs=args.correlated_pairs,
        high_missing_cols=args.high_missing_cols,
        output_dir=args.output_dir,
        base_name=args.base_name,
    )
