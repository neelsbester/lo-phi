# Lo-phi Development Makefile
# Usage: make <target>

.PHONY: all test test-unit test-integration test-verbose lint fmt check-fmt check build release clean gen-test-data help

# Default target
all: check

# Run all tests
test:
	cargo test --all-features

# Run only unit tests (library tests)
test-unit:
	cargo test --lib --all-features

# Run only integration tests
test-integration:
	cargo test --test '*' --all-features

# Run tests with output visible (for debugging)
test-verbose:
	cargo test --all-features -- --nocapture

# Run a specific test by name
# Usage: make test-one TEST=test_name
test-one:
	cargo test --all-features $(TEST) -- --nocapture

# Lint with clippy
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Format code
fmt:
	cargo fmt

# Check formatting without modifying
check-fmt:
	cargo fmt -- --check

# Full CI check locally (format + lint + test)
check: check-fmt lint test
	@echo ""
	@echo "✓ All checks passed!"

# Build debug binary
build:
	cargo build

# Build release binary
release:
	cargo build --release

# Clean build artifacts
clean:
	cargo clean

# Generate small test data for development
gen-test-data:
	python scripts/generate_test_data.py \
		--rows 1000 \
		--num-cols 50 \
		--cat-cols 10 \
		--correlated-pairs 5 \
		--high-missing-cols 3 \
		--base-name small_test

# Generate large test data for benchmarking
gen-test-data-large:
	python scripts/generate_test_data.py \
		--rows 100000 \
		--num-cols 500 \
		--cat-cols 50 \
		--correlated-pairs 20 \
		--high-missing-cols 10 \
		--base-name large_test

# Run the tool on test data
run-test:
	cargo run -- -i test_data/small_test.parquet -t target --no-confirm

# Watch for changes and run tests (requires cargo-watch)
watch:
	cargo watch -x test

# Show test coverage (requires cargo-tarpaulin)
coverage:
	cargo tarpaulin --out Html --output-dir coverage

# Update dependencies
update:
	cargo update

# Check for outdated dependencies (requires cargo-outdated)
outdated:
	cargo outdated

# Show help
help:
	@echo "Lo-phi Development Commands"
	@echo ""
	@echo "Testing:"
	@echo "  make test            - Run all tests"
	@echo "  make test-unit       - Run only unit tests"
	@echo "  make test-integration- Run only integration tests"
	@echo "  make test-verbose    - Run tests with output"
	@echo "  make test-one TEST=x - Run specific test"
	@echo ""
	@echo "Code Quality:"
	@echo "  make lint            - Run clippy linter"
	@echo "  make fmt             - Format code"
	@echo "  make check-fmt       - Check formatting"
	@echo "  make check           - Full CI check (fmt + lint + test)"
	@echo ""
	@echo "Building:"
	@echo "  make build           - Build debug binary"
	@echo "  make release         - Build release binary"
	@echo "  make clean           - Clean build artifacts"
	@echo ""
	@echo "Data Generation:"
	@echo "  make gen-test-data      - Generate small test dataset"
	@echo "  make gen-test-data-large- Generate large test dataset"
	@echo ""
	@echo "Other:"
	@echo "  make run-test        - Run tool on test data"
	@echo "  make watch           - Watch and run tests on changes"
	@echo "  make coverage        - Generate coverage report"
	@echo "  make help            - Show this help"

