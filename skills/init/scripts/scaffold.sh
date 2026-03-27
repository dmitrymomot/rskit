#!/usr/bin/env bash
# Creates the base modo project structure with all core files.
# Usage: scaffold.sh <project_dir> [project_name]
#
# Files created:
#   src/config.rs, src/error.rs, src/routes/health.rs
#   migrations/app/001_initial.sql
#   .gitignore, .github/workflows/ci.yml, data/.gitkeep

set -euo pipefail

PROJECT_DIR="${1:?Usage: scaffold.sh <project_dir> [project_name]}"
PROJECT_NAME="${2:-$(basename "$PROJECT_DIR")}"
CRATE_NAME=$(echo "$PROJECT_NAME" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | sed 's/[^a-z0-9-]//g')

echo "Scaffolding modo project: $CRATE_NAME"

# ── Directories ──────────────────────────────────────────────
mkdir -p "$PROJECT_DIR"/src/{routes,handlers}
mkdir -p "$PROJECT_DIR"/config
mkdir -p "$PROJECT_DIR"/migrations/app
mkdir -p "$PROJECT_DIR"/data
mkdir -p "$PROJECT_DIR"/.github/workflows

# ── src/config.rs ────────────────────────────────────────────
cat > "$PROJECT_DIR/src/config.rs" << 'RUST'
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(flatten)]
    pub modo: modo::Config,
}
RUST

# ── src/error.rs ─────────────────────────────────────────────
cat > "$PROJECT_DIR/src/error.rs" << 'RUST'
use modo::axum::http::request::Parts;
use modo::axum::response::{IntoResponse, Response};

pub async fn handle_error(err: modo::Error, _parts: Parts) -> Response {
    err.into_response()
}
RUST

# ── src/routes/health.rs ────────────────────────────────────
cat > "$PROJECT_DIR/src/routes/health.rs" << 'RUST'
use modo::axum::Router;

pub fn router() -> Router<modo::service::AppState> {
    modo::health::router()
}
RUST

# ── migrations/app/001_initial.sql ──────────────────────────
cat > "$PROJECT_DIR/migrations/app/001_initial.sql" << 'SQL'
-- Sessions table (required by modo session middleware)
CREATE TABLE IF NOT EXISTS sessions (
    token       TEXT PRIMARY KEY,
    data        TEXT    NOT NULL DEFAULT '{}',
    user_id     TEXT,
    ip_address  TEXT,
    user_agent  TEXT,
    fingerprint TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    expires_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
SQL

# ── .gitignore ───────────────────────────────────────────────
cat > "$PROJECT_DIR/.gitignore" << 'GIT'
/target
/data/*.db
/data/*.db-*
/data/*.mmdb
.env
Cargo.lock

# IDE
.idea/
.vscode/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db
GIT

# ── .editorconfig ───────────────────────────────────────────
cat > "$PROJECT_DIR/.editorconfig" << 'EDITORCONFIG'
root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true

[*.rs]
indent_style = space
indent_size = 4

[*.{yml,yaml}]
indent_style = space
indent_size = 2

[justfile]
indent_style = space
indent_size = 4

[*.md]
indent_style = space
indent_size = 2
trim_trailing_whitespace = false

[*.sql]
indent_style = space
indent_size = 4

[*.html]
indent_style = space
indent_size = 2

[*.toml]
indent_style = space
indent_size = 4

[*.css]
indent_style = space
indent_size = 2
EDITORCONFIG

# ── data/.gitkeep ────────────────────────────────────────────
touch "$PROJECT_DIR/data/.gitkeep"

# ── .github/workflows/ci.yml ────────────────────────────────
cat > "$PROJECT_DIR/.github/workflows/ci.yml" << 'YAML'
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy -- -D warnings
      - run: cargo test
YAML

echo "Base structure created at $PROJECT_DIR"
echo "Crate name: $CRATE_NAME"
