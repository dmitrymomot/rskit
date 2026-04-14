//! Flat index of every Tower Layer modo ships.
//!
//! Wiring-site ergonomics: `use modo::middlewares as mw;` then
//! `.layer(mw::session(...))`, `.layer(mw::cors(...))`, etc.
//!
//! Domain modules that expose a free constructor (e.g. `session`, `role`,
//! `tenant`) are re-exported as lower-case functions. Domain modules that
//! only expose a `Layer` struct (e.g. `Jwt`, `ApiKey`, `Tier`, `ClientIp`,
//! `Flash`, `Geo`, `TemplateContext`) are re-exported as PascalCase structs
//! — call `::new(...)` at the wiring site.

// Free constructor functions.
pub use crate::auth::role::middleware as role;
pub use crate::auth::session::layer as session;
pub use crate::tenant::middleware as tenant;

// Layer structs — users call `::new(...)`.
pub use crate::auth::apikey::ApiKeyLayer as ApiKey;
pub use crate::auth::jwt::JwtLayer as Jwt;
pub use crate::flash::FlashLayer as Flash;
pub use crate::geolocation::GeoLayer as Geo;
pub use crate::ip::ClientIpLayer as ClientIp;
pub use crate::template::TemplateContextLayer as TemplateContext;
pub use crate::tier::TierLayer as Tier;

// Always-available middleware — free functions.
pub use crate::middleware::{
    catch_panic, compression, cors, csrf, error_handler, rate_limit, request_id, security_headers,
    tracing,
};
