use include_dir::{Dir, include_dir};

static SHARED: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/shared");
static MINIMAL: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/minimal");
static API: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/api");
static WEB: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/web");
static WORKER: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates/worker");

pub fn shared() -> &'static Dir<'static> {
    &SHARED
}

pub fn get(name: &str) -> Option<&'static Dir<'static>> {
    match name {
        "minimal" => Some(&MINIMAL),
        "api" => Some(&API),
        "web" => Some(&WEB),
        "worker" => Some(&WORKER),
        _ => None,
    }
}
