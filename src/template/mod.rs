//! MiniJinja-based template rendering for modo.
//!
//! This module provides an opinionated template layer built on top of
//! [MiniJinja](https://docs.rs/minijinja). It covers:
//!
//! - [`Engine`] / [`EngineBuilder`] — compile and cache templates from disk, register
//!   custom functions and filters, manage locale resolvers, and serve static files.
//! - [`TemplateContextLayer`] — Tower middleware that injects per-request data
//!   (`locale`, `current_url`, `is_htmx`, `csrf_token`, `flash_messages`) into every
//!   request's extensions before the handler runs.
//! - [`Renderer`] — axum extractor that gives handlers a ready-to-use render handle.
//! - [`TemplateContext`] — a typed key-value map that middleware and handlers share;
//!   handler values override middleware values on key conflicts.
//! - [`HxRequest`] — infallible extractor that detects whether the request carries the
//!   `HX-Request: true` header.
//! - Locale resolution chain: [`LocaleResolver`] trait plus built-in resolvers
//!   ([`QueryParamResolver`], [`CookieResolver`], [`SessionResolver`],
//!   [`AcceptLanguageResolver`]).
//!
//! Requires the **`templates`** feature flag.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use modo::template::{Engine, EngineBuilder, TemplateConfig, TemplateContextLayer};
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
pub use locale::{AcceptLanguageResolver, CookieResolver, LocaleResolver, QueryParamResolver};
#[cfg(feature = "session")]
pub use locale::SessionResolver;
pub use middleware::TemplateContextLayer;
pub use minijinja::context;
pub use renderer::Renderer;
