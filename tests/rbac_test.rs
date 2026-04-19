use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use modo::auth::guard;
use modo::auth::role::{self, Role, RoleExtractor};
use modo::service::Registry;
use tower::ServiceExt;

struct StaticRoleExtractor {
    role: String,
}

impl RoleExtractor for StaticRoleExtractor {
    async fn extract(&self, _parts: &mut http::request::Parts) -> modo::Result<String> {
        Ok(self.role.clone())
    }
}

// Module-level handler functions (required for axum Handler bounds)
async fn ok_handler() -> &'static str {
    "ok"
}

async fn role_handler(role: Role) -> String {
    format!("role:{}", role.as_str())
}

async fn optional_role_handler(role: Option<Role>) -> String {
    match role {
        Some(r) => format!("role:{}", r.as_str()),
        None => "no-role".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Full stack: RBAC middleware + guards on real Router
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rbac_middleware_with_require_role_passes() {
    let app = Router::new()
        .route("/admin", get(ok_handler))
        .route_layer(guard::require_role(["admin"]))
        .layer(role::middleware(StaticRoleExtractor {
            role: "admin".into(),
        }))
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/admin").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn rbac_middleware_with_require_role_rejects_wrong_role() {
    let app = Router::new()
        .route("/admin", get(ok_handler))
        .route_layer(guard::require_role(["admin"]))
        .layer(role::middleware(StaticRoleExtractor {
            role: "viewer".into(),
        }))
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/admin").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rbac_middleware_unauthenticated_redirects() {
    // require_authenticated now checks Session, not Role. With no session
    // middleware wired here, Session is absent, so the guard redirects.
    // Note: no role middleware is wired either — require_authenticated is
    // independent of the role system post-Task 2.
    let app = Router::new()
        .route("/admin", get(ok_handler))
        .route_layer(guard::require_authenticated("/auth"))
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/admin").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/auth");
}

// ---------------------------------------------------------------------------
// Nested guards: group-level + route-level narrowing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nested_guards_owner_accesses_all() {
    let settings = Router::new()
        .route("/general", get(ok_handler))
        .route(
            "/danger-zone",
            get(ok_handler).route_layer(guard::require_role(["owner"])),
        )
        .route_layer(guard::require_role(["owner", "admin"]));

    let app = Router::new()
        .nest("/settings", settings)
        .layer(role::middleware(StaticRoleExtractor {
            role: "owner".into(),
        }))
        .with_state(Registry::new().into_state());

    // Owner can access general
    let resp = app
        .clone()
        .oneshot(
            Request::get("/settings/general")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Owner can access danger-zone
    let resp = app
        .oneshot(
            Request::get("/settings/danger-zone")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn nested_guards_admin_blocked_from_owner_only() {
    let settings = Router::new()
        .route("/general", get(ok_handler))
        .route(
            "/danger-zone",
            get(ok_handler).route_layer(guard::require_role(["owner"])),
        )
        .route_layer(guard::require_role(["owner", "admin"]));

    let app = Router::new()
        .nest("/settings", settings)
        .layer(role::middleware(StaticRoleExtractor {
            role: "admin".into(),
        }))
        .with_state(Registry::new().into_state());

    // Admin can access general
    let resp = app
        .clone()
        .oneshot(
            Request::get("/settings/general")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Admin blocked from danger-zone
    let resp = app
        .oneshot(
            Request::get("/settings/danger-zone")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Handler-level role checking
// ---------------------------------------------------------------------------

#[tokio::test]
async fn handler_reads_role() {
    let app = Router::new()
        .route("/whoami", get(role_handler))
        .layer(role::middleware(StaticRoleExtractor {
            role: "editor".into(),
        }))
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/whoami").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"role:editor");
}

// ---------------------------------------------------------------------------
// Optional role extraction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn optional_role_some_when_middleware_applied() {
    let app = Router::new()
        .route("/check", get(optional_role_handler))
        .layer(role::middleware(StaticRoleExtractor {
            role: "admin".into(),
        }))
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/check").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"role:admin");
}

#[tokio::test]
async fn optional_role_none_when_no_middleware() {
    let app = Router::new()
        .route("/check", get(optional_role_handler))
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/check").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"no-role");
}
