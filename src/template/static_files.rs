use std::collections::HashMap;
use std::path::Path;

use sha2::{Digest, Sha256};

pub(crate) fn compute_hashes(static_path: &Path) -> crate::Result<HashMap<String, String>> {
    let mut hashes = HashMap::new();
    if static_path.exists() {
        walk_dir(static_path, static_path, &mut hashes)?;
    }
    Ok(hashes)
}

fn walk_dir(base: &Path, dir: &Path, hashes: &mut HashMap<String, String>) -> crate::Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        crate::Error::internal(format!("Failed to read directory {}: {e}", dir.display()))
    })?;

    for entry in entries {
        let entry = entry
            .map_err(|e| crate::Error::internal(format!("Failed to read directory entry: {e}")))?;
        let path = entry.path();

        if path.is_dir() {
            walk_dir(base, &path, hashes)?;
        } else {
            let content = std::fs::read(&path).map_err(|e| {
                crate::Error::internal(format!("Failed to read {}: {e}", path.display()))
            })?;

            let mut hasher = Sha256::new();
            hasher.update(&content);
            let hash = format!("{:x}", hasher.finalize());
            let short_hash = hash[..8].to_string();

            let relative = path
                .strip_prefix(base)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();

            hashes.insert(relative, short_hash);
        }
    }

    Ok(())
}

pub(crate) fn build_static_url(
    prefix: &str,
    hashes: &HashMap<String, String>,
    path: &str,
) -> String {
    if let Some(hash) = hashes.get(path) {
        format!("{prefix}/{path}?v={hash}")
    } else {
        format!("{prefix}/{path}")
    }
}

pub(crate) fn make_static_url_function(
    prefix: String,
    hashes: HashMap<String, String>,
) -> impl Fn(String) -> String + Send + Sync + 'static {
    move |path: String| build_static_url(&prefix, &hashes, &path)
}

pub(crate) fn static_service(static_path: &str, prefix: &str) -> axum::Router {
    use tower_http::services::ServeDir;

    let serve = ServeDir::new(static_path);

    // In debug mode: no-cache. In release: immutable cache.
    if cfg!(debug_assertions) {
        axum::Router::new().nest_service(
            prefix,
            tower::ServiceBuilder::new()
                .layer(axum::middleware::from_fn(no_cache_middleware))
                .service(serve),
        )
    } else {
        axum::Router::new().nest_service(
            prefix,
            tower::ServiceBuilder::new()
                .layer(axum::middleware::from_fn(immutable_cache_middleware))
                .service(serve),
        )
    }
}

async fn no_cache_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-cache"),
    );
    resp
}

async fn immutable_cache_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_static_file(dir: &Path, path: &str, content: &str) {
        let full_path = dir.join(path);
        std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
        std::fs::write(full_path, content).unwrap();
    }

    #[test]
    fn computes_hashes_for_all_files() {
        let dir = tempfile::tempdir().unwrap();
        write_static_file(dir.path(), "css/app.css", "body { color: red; }");
        write_static_file(dir.path(), "js/app.js", "console.log('hello');");

        let map = compute_hashes(dir.path()).unwrap();
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("css/app.css"));
        assert!(map.contains_key("js/app.js"));
    }

    #[test]
    fn hash_is_8_hex_chars() {
        let dir = tempfile::tempdir().unwrap();
        write_static_file(dir.path(), "style.css", "body {}");

        let map = compute_hashes(dir.path()).unwrap();
        let hash = &map["style.css"];
        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn same_content_produces_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        write_static_file(dir.path(), "a.css", "same");
        write_static_file(dir.path(), "b.css", "same");

        let map = compute_hashes(dir.path()).unwrap();
        assert_eq!(map["a.css"], map["b.css"]);
    }

    #[test]
    fn different_content_produces_different_hash() {
        let dir = tempfile::tempdir().unwrap();
        write_static_file(dir.path(), "a.css", "aaa");
        write_static_file(dir.path(), "b.css", "bbb");

        let map = compute_hashes(dir.path()).unwrap();
        assert_ne!(map["a.css"], map["b.css"]);
    }

    #[test]
    fn static_url_generates_versioned_path() {
        let mut hashes = HashMap::new();
        hashes.insert("css/app.css".into(), "a3f2b1c4".into());

        let url = build_static_url("/assets", &hashes, "css/app.css");
        assert_eq!(url, "/assets/css/app.css?v=a3f2b1c4");
    }

    #[test]
    fn static_url_returns_plain_path_for_unknown_file() {
        let hashes = HashMap::new();
        let url = build_static_url("/assets", &hashes, "unknown.css");
        assert_eq!(url, "/assets/unknown.css");
    }

    #[test]
    fn empty_directory_produces_empty_map() {
        let dir = tempfile::tempdir().unwrap();
        let map = compute_hashes(dir.path()).unwrap();
        assert!(map.is_empty());
    }
}
