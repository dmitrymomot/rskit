use include_dir::{Dir, include_dir};

static SHARED: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/shared");
static MINIMAL: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/minimal");
static API: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/api");
static WEB: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/web");
static WORKER: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/worker");

/// Returns the shared template directory, which is applied to every project regardless of template.
pub fn shared() -> &'static Dir<'static> {
    &SHARED
}

/// Returns the embedded template directory for `name`, or `None` if `name` is not recognised.
///
/// Valid names are `"minimal"`, `"api"`, `"web"`, and `"worker"`.
pub fn get(name: &str) -> Option<&'static Dir<'static>> {
    match name {
        "minimal" => Some(&MINIMAL),
        "api" => Some(&API),
        "web" => Some(&WEB),
        "worker" => Some(&WORKER),
        _ => None,
    }
}
