//! Flat index of every Tower Layer modo ships.
//!
//! Wiring-site ergonomics: `use modo::middlewares as mw;` then
//! `.layer(mw::session(...))`, `.layer(mw::cors(...))`, etc.
//!
//! Two calling conventions are exported, matching how the underlying
//! domain modules construct their layers:
//!
//! - **lower_case names are functions** — call them directly:
//!   `mw::session(store, cookie_cfg, key)`, `mw::role(extractor)`,
//!   `mw::tenant(strategy, resolver)`, `mw::cors(cors_cfg)`.
//! - **PascalCase names are `Layer` structs** — call `::new(...)`:
//!   `mw::Jwt::new(cfg)`, `mw::ApiKey::new(store)`,
//!   `mw::Flash::new(cookie_cfg)`, `mw::ClientIp::new(trusted_proxies)`.
//!
//! The split reflects upstream constructor design (some modules expose
//! free constructors, others only their `Layer` type). It avoids
//! inventing wrapper functions just for uniformity.

// Free constructor functions.
pub use crate::auth::role::middleware as role;
// NOTE: session layer constructor removed in v0.8 — SessionStore is now pub(crate);
// use modo::auth::session::layer directly within-crate, or via TestSession in tests.
pub use crate::tenant::middleware as tenant;

// Layer structs — users call `::new(...)`.
pub use crate::auth::apikey::ApiKeyLayer as ApiKey;
pub use crate::auth::session::jwt::JwtLayer as Jwt;
pub use crate::flash::FlashLayer as Flash;
pub use crate::geolocation::GeoLayer as Geo;
pub use crate::i18n::I18nLayer as I18n;
pub use crate::ip::ClientIpLayer as ClientIp;
pub use crate::template::TemplateContextLayer as TemplateContext;
pub use crate::tier::TierLayer as Tier;

// Always-available middleware — free functions.
pub use crate::middleware::{
    catch_panic, compression, cors, csrf, default_error_handler, error_handler, rate_limit,
    request_id, security_headers, tracing,
};
