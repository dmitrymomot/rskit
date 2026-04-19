#!/usr/bin/env bash
# Adds background jobs component files to a modo project.
# Usage: init_jobs.sh <project_dir>
#
# Creates: migrations/jobs/001_jobs.sql, src/jobs/mod.rs, src/jobs/example.rs

set -euo pipefail

PROJECT_DIR="${1:?Usage: init_jobs.sh <project_dir>}"

mkdir -p "$PROJECT_DIR"/migrations/jobs
mkdir -p "$PROJECT_DIR"/src/jobs

# ── migrations/jobs/001_jobs.sql ─────────────────────────────
cat > "$PROJECT_DIR/migrations/jobs/001_jobs.sql" << 'SQL'
-- Job queue table
CREATE TABLE IF NOT EXISTS jobs (
    id            TEXT    PRIMARY KEY,
    name          TEXT    NOT NULL,
    queue         TEXT    NOT NULL DEFAULT 'default',
    payload       TEXT    NOT NULL DEFAULT '{}',
    payload_hash  TEXT,
    status        TEXT    NOT NULL DEFAULT 'pending',
    attempt       INTEGER NOT NULL DEFAULT 0,
    run_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    started_at    TEXT,
    completed_at  TEXT,
    failed_at     TEXT,
    error_message TEXT,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_jobs_status_queue_run_at
    ON jobs(status, queue, run_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_payload_hash_pending
    ON jobs(payload_hash) WHERE payload_hash IS NOT NULL AND status IN ('pending', 'running');
SQL

# ── src/jobs/mod.rs ──────────────────────────────────────────
cat > "$PROJECT_DIR/src/jobs/mod.rs" << 'RUST'
pub mod example;
RUST

# ── src/jobs/example.rs ─────────────────────────────────────
cat > "$PROJECT_DIR/src/jobs/example.rs" << 'RUST'
use modo::job::{Meta, Payload};
use modo::Result;

pub async fn handle(payload: Payload<String>, meta: Meta) -> Result<()> {
    modo::tracing::info!(payload = %payload.0, job_id = %meta.id, "processing example job");
    Ok(())
}

pub async fn scheduled() -> Result<()> {
    modo::tracing::info!("hourly cron job running");
    Ok(())
}
RUST

echo "Jobs component added"
