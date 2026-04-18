//! # modo::template
//!
//! MiniJinja-based template rendering for modo.
//!
//! This module provides an opinionated template layer built on top of
//! [MiniJinja](https://docs.rs/minijinja). It covers engine construction,
//! per-request context injection, HTMX-aware rendering, and cache-busted
//! static-asset URLs.
//!
//! For internationalization, see [`modo::i18n`](crate::i18n). Pass an
//! [`I18n`](crate::i18n::I18n) handle to [`EngineBuilder::i18n`] to register
//! the `t()` template function.
//!
//! ## Provides
//!
//! | Type / Trait                | Description |
//! |-----------------------------|-------------|
//! | [`Engine`] / [`EngineBuilder`] | Compile and cache templates from disk and register custom functions and filters. |
//! | [`TemplateConfig`]          | Configuration for template paths and static-asset prefix. |
//! | [`TemplateContext`]         | Per-request key-value map shared between middleware and handlers; handler values override middleware values on key conflicts. |
//! | [`TemplateContextLayer`]    | Tower middleware that injects per-request data (`current_url`, `is_htmx`, `request_id`, `locale`, `csrf_token`, `flash_messages`, and `tier_*` entries) into every request's extensions. Also re-exported as [`modo::middlewares::TemplateContext`](crate::middlewares::TemplateContext). |
//! | [`Renderer`]                | Axum extractor that gives handlers a ready-to-use render handle. |
//! | [`HxRequest`]               | Infallible axum extractor that detects the `HX-Request: true` header. Also re-exported from [`modo::extractors`](crate::extractors). |
//! | [`context`]                 | Re-export of [`minijinja::context!`] for building template data in handlers. |
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::template::{Engine, TemplateConfig, TemplateContextLayer};
//!
//! // Build the engine once at startup.
//! let engine = Engine::builder()
//!     .config(TemplateConfig::default())
//!     .build()
//!     .expect("failed to build engine");
//!
//! // Serve static files and inject per-request context.
//! // `Engine` is cheaply cloneable (internal `Arc`).
//! let router: axum::Router = axum::Router::new()
//!     .merge(engine.static_service())
//!     .layer(TemplateContextLayer::new());
//! ```

mod config;
mod context;
mod engine;
mod htmx;
mod middleware;
mod renderer;
mod static_files;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use engine::{Engine, EngineBuilder};
pub use htmx::HxRequest;
pub use middleware::TemplateContextLayer;
pub use minijinja::context;
pub use renderer::Renderer;
