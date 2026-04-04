# List available recipes
default:
    @just --list

# Format all code
fmt:
    cargo fmt

# Check formatting (CI-friendly, fails on diff)
fmt-check:
    cargo fmt --check

# Lint with all features enabled
lint:
    cargo clippy --all-features --tests -- -D warnings

# Run tests (default features only)
test:
    cargo test

# Run tests with all features enabled
test-all:
    cargo test --all-features

# Run all checks (fmt + lint + test-all) — use in CI or pre-push
check: fmt-check lint test-all

# Install git hooks (run once after clone)
setup:
    git config core.hooksPath .githooks
