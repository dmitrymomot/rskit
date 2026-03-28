use crate::error::{Error, Result};

/// Run SQL migrations from a directory against a connection.
///
/// Reads `*.sql` files sorted by filename, tracks applied migrations
/// in a `_migrations` table with checksum verification.
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

    // Read and sort migration files
    let dir_path = std::path::Path::new(dir);
    if !dir_path.exists() {
        return Ok(()); // No migrations directory — nothing to do
    }

    let mut files: Vec<std::fs::DirEntry> = std::fs::read_dir(dir_path)
        .map_err(|e| {
            Error::internal(format!("failed to read migrations directory: {dir}")).chain(e)
        })?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "sql")
        })
        .collect();
    files.sort_by_key(|e| e.file_name());

    for entry in files {
        let name = entry.file_name().to_string_lossy().to_string();
        let sql = std::fs::read_to_string(entry.path()).map_err(|e| {
            Error::internal(format!("failed to read migration file: {name}")).chain(e)
        })?;
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

        // Apply migration
        conn.execute_batch(&sql).await.map_err(|e| {
            Error::internal(format!("failed to apply migration '{name}'")).chain(e)
        })?;

        conn.execute(
            "INSERT INTO _migrations (name, checksum) VALUES (?1, ?2)",
            libsql::params![name.clone(), checksum],
        )
        .await
        .map_err(Error::from)?;
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
