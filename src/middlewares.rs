//! Flat index of every Tower Layer modo ships.
//!
//! Wiring-site ergonomics: `use modo::middlewares as mw;` then
//! `.layer(mw::cors(cfg))`, `.layer(mw::role(extractor))`, `.layer(mw::Flash::new(cfg))`, etc.
//!
//! # Name shadowing with [`crate::prelude`]
//!
//! Some layer names in this module deliberately re-export a type under the
//! same name used by a factory in [`crate::prelude`]. For example,
//! [`crate::prelude::I18n`] is the factory ([`crate::i18n::I18n`]) while
//! [`I18n`] in this module is the Tower layer ([`crate::i18n::I18nLayer`]).
//! [`Flash`] has the same split ([`crate::flash::Flash`] vs
//! [`crate::flash::FlashLayer`]).
//!
//! A file that does both `use modo::prelude::*;` and
//! `use modo::middlewares::*;` at once will silently shadow one with the
//! other — whichever `use` came second wins, with no compiler warning. The
//! recommended convention is `use modo::middlewares as mw;` so layer names
//! sit behind the `mw::` prefix and never collide with prelude items.
//!
//! # Calling conventions
//!
//! Two calling conventions are exported, matching how the underlying
//! domain modules construct their layers:
//!
//! - **lower_case names are functions** — call them directly:
//!   `mw::role(extractor)`, `mw::tenant(strategy, resolver)`,
//!   `mw::cors(cors_cfg)`, `mw::csrf(csrf_cfg)`.
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
#[doc(alias = "FlashLayer")]
pub use crate::flash::FlashLayer as Flash;
pub use crate::geolocation::GeoLayer as Geo;
#[doc(alias = "I18nLayer")]
pub use crate::i18n::I18nLayer as I18n;
pub use crate::ip::ClientIpLayer as ClientIp;
pub use crate::template::TemplateContextLayer as TemplateContext;
pub use crate::tier::TierLayer as Tier;

// Always-available middleware — free functions.
pub use crate::middleware::{
    catch_panic, compression, cors, csrf, default_error_handler, error_handler, rate_limit,
    request_id, security_headers, tracing,
};
