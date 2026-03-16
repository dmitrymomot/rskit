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

    # Macro crates with circular dev-deps need --no-verify because their
    # dev-dependencies (e.g. modo-db for modo-db-macros) aren't published yet
    # at the point they go out. --no-verify skips the build check that would
    # try to resolve those dev-deps from crates.io.
    NO_VERIFY_CRATES="modo-db-macros modo-jobs-macros modo-upload-macros"

    # Topological order based on normal (non-dev) dependencies:
    #   1. leaf macro crates (no workspace normal deps)
    #   2. modo (depends on modo-macros)
    #   3. crates depending on modo + their macro crate
    #   4. crates depending on multiple lower-level crates
    CRATES=(
        modo-macros
        modo-db-macros
        modo-jobs-macros
        modo-upload-macros
        modo-cli
        modo
        modo-db
        modo-email
        modo-tenant
        modo-jobs
        modo-upload
        modo-session
        modo-auth
    )

    for name in "${CRATES[@]}"; do
        echo "Publishing $name..."
        flags=""
        if echo "$NO_VERIFY_CRATES" | grep -qw "$name"; then
            flags="--no-verify"
        fi
        if ! output=$(cargo publish -p "$name" $flags 2>&1); then
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
