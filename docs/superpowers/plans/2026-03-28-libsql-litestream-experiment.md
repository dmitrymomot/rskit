# libSQL + Litestream Backup Experiment — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Validate that Litestream can back up and restore a libSQL database (with vector data) to S3-compatible storage (RustFS).

**Architecture:** Standalone mini Rust app in `experiments/libsql/` creates a libSQL database with vector columns, inserts sample data, and queries via `vector_top_k`. Docker Compose runs RustFS for S3 storage. Host-installed Litestream replicates the database. Verification: delete DB, restore from RustFS, confirm data intact.

**Tech Stack:** libsql 0.9, tokio, Docker (RustFS + minio/mc), Litestream (host CLI)

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `.gitignore` | Modify | Add `experiments/` |
| `experiments/libsql/Cargo.toml` | Create | Mini app dependencies |
| `experiments/libsql/src/main.rs` | Create | DB setup, insert vectors, query, print |
| `experiments/libsql/docker-compose.yml` | Create | RustFS + bucket init |
| `experiments/libsql/litestream.yml` | Create | Litestream replication config |
| `experiments/libsql/data/` | Create | Directory for app.db (empty, created at runtime) |

---

### Task 1: Scaffold experiment directory and gitignore

**Files:**
- Modify: `.gitignore`
- Create: `experiments/libsql/data/.gitkeep`

- [ ] **Step 1: Add `experiments/` to `.gitignore`**

Append to the root `.gitignore`:

```
experiments/
```

- [ ] **Step 2: Create the experiment directory structure**

```bash
mkdir -p experiments/libsql/src experiments/libsql/data
touch experiments/libsql/data/.gitkeep
```

- [ ] **Step 3: Verify**

```bash
ls experiments/libsql/
```

Expected: `data/` and `src/` directories exist.

---

### Task 2: Create docker-compose.yml

**Files:**
- Create: `experiments/libsql/docker-compose.yml`

- [ ] **Step 1: Write docker-compose.yml**

Create `experiments/libsql/docker-compose.yml`:

```yaml
services:
  rustfs:
    image: rustfs/rustfs:latest
    ports:
      - "9000:9000"
      - "9001:9001"
    environment:
      RUSTFS_ACCESS_KEY: admin
      RUSTFS_SECRET_KEY: admin123
    volumes:
      - rustfs_data:/data

  rustfs-bucket-init:
    image: minio/mc:latest
    depends_on:
      - rustfs
    entrypoint: >
      /bin/sh -c "
      sleep 3;
      mc alias set rustfs http://rustfs:9000 admin admin123;
      mc mb --ignore-existing rustfs/backups;
      exit 0;
      "

volumes:
  rustfs_data:
```

- [ ] **Step 2: Verify Docker Compose parses correctly**

```bash
cd experiments/libsql && docker compose config
```

Expected: Valid YAML output with both services and the volume listed, no errors.

---

### Task 3: Create litestream.yml

**Files:**
- Create: `experiments/libsql/litestream.yml`

- [ ] **Step 1: Write litestream.yml**

Create `experiments/libsql/litestream.yml`:

```yaml
dbs:
  - path: ./data/app.db
    replicas:
      - type: s3
        bucket: backups
        path: libsql-experiment/
        endpoint: http://localhost:9000
        access-key-id: admin
        secret-access-key: admin123
        force-path-style: true
```

- [ ] **Step 2: Verify litestream can parse the config**

```bash
cd experiments/libsql && litestream databases -config litestream.yml
```

Expected: Lists `./data/app.db` as a monitored database (may warn it doesn't exist yet — that's fine).

---

### Task 4: Create Cargo.toml

**Files:**
- Create: `experiments/libsql/Cargo.toml`

- [ ] **Step 1: Write Cargo.toml**

Create `experiments/libsql/Cargo.toml`:

```toml
[package]
name = "libsql-experiment"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
libsql = "0.9"
tokio = { version = "1", features = ["full"] }
```

- [ ] **Step 2: Verify dependencies resolve**

```bash
cd experiments/libsql && cargo check
```

Expected: Compiles successfully (main.rs doesn't exist yet — create a placeholder first or expect a compile error about missing main. To avoid that, create a minimal `src/main.rs` with `fn main() {}` first, then run `cargo check`).

Revised step:

```bash
cd experiments/libsql && echo 'fn main() {}' > src/main.rs && cargo check
```

Expected: Dependencies download and compile. `Finished` with no errors.

---

### Task 5: Write the mini app

**Files:**
- Create: `experiments/libsql/src/main.rs`

- [ ] **Step 1: Write src/main.rs**

Create `experiments/libsql/src/main.rs`:

```rust
use libsql::params;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Open or create the database
    let db = libsql::Builder::new_local("data/app.db")
        .build()
        .await?;
    let conn = db.connect()?;
    println!("Connected to data/app.db");

    // 2. Create table with vector column
    conn.execute(
        "CREATE TABLE IF NOT EXISTS docs (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            embedding F32_BLOB(4)
        )",
        (),
    )
    .await?;
    println!("Table 'docs' ready");

    // 3. Insert sample documents (idempotent)
    let samples = [
        ("doc_1", "Rust is a systems language", "[1.0, 0.5, 0.3, 0.8]"),
        ("doc_2", "SQLite is an embedded database", "[0.2, 1.0, 0.7, 0.4]"),
        ("doc_3", "Vectors enable similarity search", "[0.9, 0.4, 0.6, 1.0]"),
        ("doc_4", "Litestream replicates SQLite", "[0.3, 0.8, 0.9, 0.2]"),
        ("doc_5", "RustFS is S3-compatible storage", "[0.1, 0.3, 0.5, 0.7]"),
    ];

    for (id, content, embedding) in &samples {
        conn.execute(
            "INSERT OR IGNORE INTO docs (id, content, embedding) VALUES (?1, ?2, vector(?3))",
            params![*id, *content, *embedding],
        )
        .await?;
    }
    println!("Inserted {} sample documents", samples.len());

    // 4. Create vector index
    conn.execute(
        "CREATE INDEX IF NOT EXISTS docs_idx ON docs (libsql_vector_idx(embedding))",
        (),
    )
    .await?;
    println!("Vector index 'docs_idx' ready");

    // 5. Count total rows
    let mut rows = conn
        .query("SELECT COUNT(*) FROM docs", ())
        .await?;
    let count: i64 = if let Some(row) = rows.next().await? {
        row.get(0)?
    } else {
        0
    };
    println!("Total documents: {count}");

    // 6. Vector similarity search
    println!("\n--- Vector search: top 3 nearest to [1,2,3,4] ---");
    let mut results = conn
        .query(
            "SELECT id, content FROM vector_top_k('docs_idx', vector('[1,2,3,4]'), 3)
             JOIN docs ON docs.rowid = id",
            (),
        )
        .await?;

    let mut result_count = 0;
    while let Some(row) = results.next().await? {
        let id: String = row.get(0)?;
        let content: String = row.get(1)?;
        println!("  {id}: {content}");
        result_count += 1;
    }
    println!("Returned {result_count} results");

    println!("\nDone.");
    Ok(())
}
```

- [ ] **Step 2: Build the app**

```bash
cd experiments/libsql && cargo build
```

Expected: Compiles with no errors.

- [ ] **Step 3: Run the app (first run — creates DB and inserts data)**

```bash
cd experiments/libsql && cargo run
```

Expected output (approximate):

```
Connected to data/app.db
Table 'docs' ready
Inserted 5 sample documents
Vector index 'docs_idx' ready
Total documents: 5

--- Vector search: top 3 nearest to [1,2,3,4] ---
  doc_3: Vectors enable similarity search
  doc_1: Rust is a systems language
  doc_5: RustFS is S3-compatible storage
Returned 3 results

Done.
```

The exact order of results may vary depending on distance calculations. The key check: 5 documents, 3 results from vector search, no errors.

- [ ] **Step 4: Run again (idempotent — same output)**

```bash
cd experiments/libsql && cargo run
```

Expected: Same output as step 3. `INSERT OR IGNORE` prevents duplicates. Count is still 5.

---

### Task 6: End-to-end Litestream backup and restore verification

This task is manual verification. Run these steps in order.

**Prerequisites:** Docker running, `litestream` CLI installed on host.

- [ ] **Step 1: Start RustFS**

```bash
cd experiments/libsql && docker compose up -d
```

Expected: Both `rustfs` and `rustfs-bucket-init` start. Wait a few seconds for the bucket init to complete.

Verify bucket exists:

```bash
docker compose logs rustfs-bucket-init
```

Expected: Log shows `Bucket created successfully` or `already exists`.

- [ ] **Step 2: Clean any previous state**

```bash
cd experiments/libsql && rm -f data/app.db data/app.db-wal data/app.db-shm
```

- [ ] **Step 3: Run the app to create fresh database**

```bash
cd experiments/libsql && cargo run
```

Expected: Same output as Task 5 Step 3. Database file `data/app.db` now exists.

- [ ] **Step 4: Start Litestream replication**

In a **separate terminal**:

```bash
cd experiments/libsql && litestream replicate -config litestream.yml
```

Expected: Litestream starts, prints replication info. No errors. Leave it running.

- [ ] **Step 5: Wait for initial replication**

Wait ~5 seconds for Litestream to complete the initial snapshot replication to RustFS.

- [ ] **Step 6: Stop Litestream**

Press `Ctrl+C` in the Litestream terminal.

- [ ] **Step 7: Delete the local database**

```bash
cd experiments/libsql && rm -f data/app.db data/app.db-wal data/app.db-shm
```

Verify it's gone:

```bash
ls -la experiments/libsql/data/
```

Expected: Only `.gitkeep` remains.

- [ ] **Step 8: Restore from RustFS**

```bash
cd experiments/libsql && litestream restore -config litestream.yml data/app.db
```

Expected: Restore completes without errors. `data/app.db` file reappears.

- [ ] **Step 9: Run the app on restored database**

```bash
cd experiments/libsql && cargo run
```

**PASS criteria**: Output matches Step 3 — same 5 documents, same 3 vector search results, no errors.

**FAIL criteria**: Any of these indicate the experiment failed:
- Restore command errors
- App crashes reading the restored DB
- Row count differs from 5
- Vector search returns different results or errors
- `vector_top_k` function not recognized (indicates vector index was lost)

- [ ] **Step 10: Cleanup**

```bash
cd experiments/libsql && docker compose down -v
rm -f data/app.db data/app.db-wal data/app.db-shm
```

---

## Summary

| Task | What | Est. Time |
|------|------|-----------|
| 1 | Scaffold dirs + gitignore | 1 min |
| 2 | docker-compose.yml | 2 min |
| 3 | litestream.yml | 1 min |
| 4 | Cargo.toml + dependency check | 2 min |
| 5 | Write and test mini app | 5 min |
| 6 | End-to-end backup/restore verification | 5 min |
