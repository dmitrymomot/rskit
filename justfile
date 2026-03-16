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

# Run all tests with postgres feature (requires running Postgres)
test-pg:
    cargo test --workspace --all-targets --features postgres

# Run all checks (fmt + lint + test) — use in CI or pre-push
check: fmt-check lint test

# Publish all crates to crates.io in dependency order
publish:
    #!/usr/bin/env bash
    set -e
    CRATES=(
        modo-macros
        modo-db-macros
        modo-jobs-macros
        modo-upload-macros
        modo
        modo-db
        modo-email
        modo-tenant
        modo-upload
        modo-jobs
        modo-session
        modo-auth
        modo-cli
    )
    for name in "${CRATES[@]}"; do
        echo "Publishing $name..."
        if ! output=$(cargo publish -p "$name" 2>&1); then
            if echo "$output" | grep -q "already exists"; then
                echo "  $name already up-to-date — skipping"
                continue
            fi
            echo "$output" >&2
            exit 1
        fi
        echo "  $name published"
        sleep 5
    done
    echo "Done."
