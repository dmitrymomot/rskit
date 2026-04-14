//! # modo::template
//!
//! MiniJinja-based template rendering for modo.
//!
//! This module provides an opinionated template layer built on top of
//! [MiniJinja](https://docs.rs/minijinja). It covers:
//!
//! ## Provided types
//!
//! | Type / Trait                | Description |
//! |-----------------------------|-------------|
//! | [`Engine`] / [`EngineBuilder`] | Compile and cache templates from disk, register custom functions and filters, manage locale resolvers, and serve static files. |
//! | [`TemplateConfig`]          | Configuration for template paths, static-asset prefix, locale defaults, and cookie/query-param names. |
//! | [`TemplateContext`]         | Per-request key-value map shared between middleware and handlers; handler values override middleware values on key conflicts. |
//! | [`TemplateContextLayer`]    | Tower middleware that injects per-request data (`locale`, `current_url`, `is_htmx`, `csrf_token`, `flash_messages`) into every request's extensions before the handler runs. |
//! | [`Renderer`]                | Axum extractor that gives handlers a ready-to-use render handle. |
//! | [`HxRequest`]               | Infallible axum extractor that detects the `HX-Request: true` header. |
//! | [`context`]                 | Re-export of [`minijinja::context!`] for building template data in handlers. |
//! | [`LocaleResolver`]          | Trait for pluggable locale detection from a request. |
//! | [`QueryParamResolver`]      | Resolves the active locale from a URL query parameter. |
//! | [`CookieResolver`]          | Resolves the active locale from a cookie. |
//! | [`AcceptLanguageResolver`]  | Resolves the active locale from the `Accept-Language` header. |
//! | `SessionResolver`           | Resolves the active locale from session data. |
//!
//! # Quick start
//!
//! ```rust,no_run
//! use modo::template::{Engine, TemplateConfig, TemplateContextLayer};
//!
//! // Build the engine once at startup
//! let engine = Engine::builder()
//!     .config(TemplateConfig::default())
//!     .build()
//!     .expect("failed to build engine");
//!
//! // Serve static files and inject per-request context
//! // let router = axum::Router::new()
//! //     .merge(engine.static_service())
//! //     .layer(TemplateContextLayer::new(engine.clone()));
//! ```

mod config;
mod context;
mod engine;
mod htmx;
mod i18n;
mod locale;
mod middleware;
mod renderer;
mod static_files;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use engine::{Engine, EngineBuilder};
pub use htmx::HxRequest;
pub use locale::SessionResolver;
pub use locale::{AcceptLanguageResolver, CookieResolver, LocaleResolver, QueryParamResolver};
pub use middleware::TemplateContextLayer;
pub use minijinja::context;
pub use renderer::Renderer;
