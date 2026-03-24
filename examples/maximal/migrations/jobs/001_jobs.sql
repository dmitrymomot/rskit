-- Job queue table
CREATE TABLE IF NOT EXISTS modo_jobs (
    id          TEXT PRIMARY KEY,
    name        TEXT    NOT NULL,
    queue       TEXT    NOT NULL DEFAULT 'default',
    payload     TEXT    NOT NULL DEFAULT '{}',
    status      TEXT    NOT NULL DEFAULT 'pending',
    attempts    INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    run_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    locked_at   TEXT,
    locked_by   TEXT,
    finished_at TEXT,
    last_error  TEXT,
    idempotency_key TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_jobs_status_queue_run_at
    ON modo_jobs(status, queue, run_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_idempotency_key
    ON modo_jobs(idempotency_key) WHERE idempotency_key IS NOT NULL;
