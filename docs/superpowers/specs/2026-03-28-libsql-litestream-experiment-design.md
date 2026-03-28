# libSQL + Litestream Backup Experiment

**Date**: 2026-03-28
**Goal**: Validate that Litestream can continuously back up a libSQL local database to S3-compatible storage (RustFS), and that a restore produces an identical database — including vector data.

## Context

modo is evaluating libSQL as a replacement for sqlx. Before committing, we need to confirm that libSQL's WAL format is compatible with Litestream's replication. This experiment is the smallest possible test of that assumption.

## Success Criteria

1. Mini app creates a libSQL database with a vector column (`F32_BLOB`)
2. App inserts records and runs a `vector_top_k` KNN query successfully
3. Litestream replicates the database to RustFS (S3-compatible)
4. After deleting the local database, `litestream restore` recovers it from RustFS
5. Re-running the app shows the same data — vectors, index, and all

## File Structure

```
experiments/             # added to .gitignore
└── libsql/
    ├── Cargo.toml       # depends on libsql + tokio
    ├── src/
    │   └── main.rs      # create table, insert vectors, query, print
    ├── docker-compose.yml  # rustfs + bucket init
    ├── litestream.yml   # litestream config (app.db → rustfs/backups)
    └── data/            # app.db lives here (gitignored via parent)
```

## Components

### 1. RustFS (Docker)

S3-compatible object storage. Stores Litestream replicas.

```yaml
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

### 2. Litestream (Host CLI)

Runs on macOS via locally installed `litestream` binary. Watches `data/app.db` and replicates to RustFS.

```yaml
# litestream.yml
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

### 3. Mini Rust App (Host)

Standalone binary. Uses `libsql` crate in local-only mode.

**Dependencies**:
- `libsql` — database driver
- `tokio` — async runtime

**App logic** (`src/main.rs`):

1. Open or create `data/app.db` via `libsql::Builder::new_local`
2. Create table:
   ```sql
   CREATE TABLE IF NOT EXISTS docs (
     id TEXT PRIMARY KEY,
     content TEXT NOT NULL,
     embedding F32_BLOB(4)
   )
   ```
3. Insert 5 sample documents with 4-dimensional vectors (idempotent via `INSERT OR IGNORE`)
4. Create vector index:
   ```sql
   CREATE INDEX IF NOT EXISTS docs_idx ON docs (libsql_vector_idx(embedding))
   ```
5. Run KNN query:
   ```sql
   SELECT id, content FROM vector_top_k('docs_idx', vector('[1,2,3,4]'), 3)
     JOIN docs ON docs.rowid = id
   ```
6. Print results and total row count

Sample data:
| id | content | embedding |
|----|---------|-----------|
| doc_1 | Rust is a systems language | [1.0, 0.5, 0.3, 0.8] |
| doc_2 | SQLite is an embedded database | [0.2, 1.0, 0.7, 0.4] |
| doc_3 | Vectors enable similarity search | [0.9, 0.4, 0.6, 1.0] |
| doc_4 | Litestream replicates SQLite | [0.3, 0.8, 0.9, 0.2] |
| doc_5 | RustFS is S3-compatible storage | [0.1, 0.3, 0.5, 0.7] |

## Verification Steps

```bash
# 1. Start RustFS
cd experiments/libsql
docker compose up -d

# 2. Start Litestream (in a separate terminal)
litestream replicate -config litestream.yml

# 3. Run the app — creates DB, inserts data, queries vectors
cargo run

# 4. Wait ~5 seconds for Litestream to replicate

# 5. Stop Litestream (Ctrl+C), then delete the database
rm -f data/app.db data/app.db-wal data/app.db-shm

# 6. Restore from RustFS
litestream restore -config litestream.yml data/app.db

# 7. Run the app again — should show same data
cargo run

# 8. Cleanup
docker compose down -v
```

**Pass**: Step 7 output matches step 3 output.
**Fail**: Restore errors, missing data, or corrupted vectors.

## Out of Scope

- Performance benchmarking
- Concurrent write testing
- Production deployment patterns
- Turso cloud / remote replicas
- FTS5 or Tantivy full-text search
# Test change for skill evaluation

## Additional Notes

This section captures additional observations from the experiment.
