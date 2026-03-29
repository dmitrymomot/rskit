use crate::error::{Error, Result};

/// Run SQL migrations from a directory against a connection.
///
/// Reads `*.sql` files sorted by filename, tracks applied migrations
/// in a `_migrations` table with checksum verification. Each migration
/// is applied inside a transaction so the schema change and the
/// `_migrations` record are committed atomically.
///
/// Already-applied migrations are skipped. If a file's checksum differs
/// from the recorded checksum, an error is returned (the file was modified
/// after being applied).
///
/// # Errors
///
/// Returns an error if the migrations directory cannot be read, a
/// migration file cannot be parsed, a checksum mismatch is detected,
/// or a migration statement fails.
pub async fn migrate(conn: &libsql::Connection, dir: &str) -> Result<()> {
    // Create tracking table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            checksum TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await
    .map_err(Error::from)?;

    // Read and sort migration files on a blocking thread
    let dir_owned = dir.to_string();
    let files = tokio::task::spawn_blocking(move || {
        let dir_path = std::path::Path::new(&dir_owned);
        if !dir_path.exists() {
            return Ok(Vec::new());
        }

        let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir_path)
            .map_err(|e| {
                Error::internal(format!("failed to read migrations directory: {dir_owned}"))
                    .chain(e)
            })?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
            .collect();
        entries.sort_by_key(|e| e.file_name());

        let mut result: Vec<(String, String)> = Vec::with_capacity(entries.len());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            let sql = std::fs::read_to_string(entry.path()).map_err(|e| {
                Error::internal(format!("failed to read migration file: {name}")).chain(e)
            })?;
            result.push((name, sql));
        }
        Ok(result)
    })
    .await
    .map_err(|e| Error::internal("migration task panicked").chain(e))?
        as Result<Vec<(String, String)>>;

    let files = files?;
    if files.is_empty() {
        return Ok(());
    }

    for (name, sql) in &files {
        let checksum = fnv1a_hex(sql.as_bytes());

        // Check if already applied
        let mut rows = conn
            .query(
                "SELECT checksum FROM _migrations WHERE name = ?1",
                libsql::params![name.clone()],
            )
            .await
            .map_err(Error::from)?;

        if let Some(row) = rows.next().await.map_err(Error::from)? {
            let existing: String = row.get(0).map_err(Error::from)?;
            if existing != checksum {
                return Err(Error::internal(format!(
                    "migration '{name}' checksum mismatch — file was modified after applying (expected {existing}, got {checksum})"
                )));
            }
            continue; // Already applied
        }

        // Apply migration inside a transaction
        conn.execute("BEGIN", ()).await.map_err(Error::from)?;

        if let Err(e) = async {
            conn.execute_batch(sql).await.map_err(|e| {
                Error::internal(format!("failed to apply migration '{name}'")).chain(e)
            })?;

            conn.execute(
                "INSERT INTO _migrations (name, checksum) VALUES (?1, ?2)",
                libsql::params![name.clone(), checksum],
            )
            .await
            .map_err(Error::from)?;

            conn.execute("COMMIT", ()).await.map_err(Error::from)?;
            Ok::<(), Error>(())
        }
        .await
        {
            if let Err(rb_err) = conn.execute("ROLLBACK", ()).await {
                tracing::error!(error = %rb_err, "rollback failed after migration error");
            }
            return Err(e);
        }
    }

    Ok(())
}

/// FNV-1a hash, deterministic and stable across Rust versions.
fn fnv1a_hex(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}
