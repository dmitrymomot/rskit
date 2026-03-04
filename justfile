# List available recipes
default:
    @just --list

# Format all code
fmt:
    cargo fmt --all

# Check formatting (CI-friendly, fails on diff)
fmt-check:
    cargo fmt --all -- --check

# Lint with maximum warnings
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run all tests
test:
    cargo test --workspace --all-targets

# Run all checks (fmt + lint + test) — use in CI or pre-push
check: fmt-check lint test
