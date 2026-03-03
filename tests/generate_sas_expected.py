"""Generate expected-output JSON from pandas for SAS7BDAT fixture files.

Usage:
    python3 tests/generate_sas_expected.py

Requires: pandas >= 1.0
"""

import pandas as pd
import json
import os
import sys

FIXTURES_DIR = "tests/fixtures/sas7bdat"
EXPECTED_DIR = os.path.join(FIXTURES_DIR, "expected")
os.makedirs(EXPECTED_DIR, exist_ok=True)

files = sorted(f for f in os.listdir(FIXTURES_DIR) if f.endswith(".sas7bdat"))
generated = 0
errors = 0

for f in files:
    path = os.path.join(FIXTURES_DIR, f)
    out_path = os.path.join(EXPECTED_DIR, f.replace(".sas7bdat", ".json"))

    df = None
    error_info = None

    try:
        df = pd.read_sas(path, encoding="utf-8")
    except Exception:
        try:
            df = pd.read_sas(path)
        except Exception as e:
            error_info = {
                "error": str(type(e).__name__),
                "message": str(e),
            }

    if error_info is not None:
        with open(out_path, "w") as fp:
            json.dump(error_info, fp, indent=2)
        print(f"  ERROR {f}: {error_info['error']}: {error_info['message'][:80]}")
        errors += 1
        continue

    # Build metadata dict
    meta = {
        "rows": len(df),
        "columns": list(df.columns),
        "dtypes": {col: str(df[col].dtype) for col in df.columns},
        "null_counts": {col: int(df[col].isna().sum()) for col in df.columns},
    }

    with open(out_path, "w") as fp:
        json.dump(meta, fp, indent=2)

    # Save first 5 rows as CSV for value comparison
    csv_path = os.path.join(EXPECTED_DIR, f.replace(".sas7bdat", "_head.csv"))
    df.head(5).to_csv(csv_path, index=False)

    print(f"  OK    {f}: {meta['rows']} rows x {len(meta['columns'])} cols")
    generated += 1

print(f"\nDone: {generated} files generated, {errors} files with errors")
print(f"Output directory: {EXPECTED_DIR}")
