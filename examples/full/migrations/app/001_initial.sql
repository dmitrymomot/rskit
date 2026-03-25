-- Sessions table
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
