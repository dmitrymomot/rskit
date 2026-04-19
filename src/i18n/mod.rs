//! # modo::i18n
//!
//! YAML-backed internationalization with request-scoped locale resolution.
//!
//! Provides:
//! - [`I18n`] — factory that loads translations and builds the Tower layer.
//! - [`I18nConfig`] — serde-deserialised configuration with sensible defaults.
//! - [`I18nLayer`] — Tower middleware that resolves the locale and injects a
//!   [`Translator`] into request extensions.
//! - [`Translator`] — axum extractor with `t()` / `t_plural()` helpers.
//! - [`TranslationStore`] — `Arc`-wrapped in-memory store of loaded entries.
//! - [`LocaleResolver`] trait plus built-in resolvers [`QueryParamResolver`],
//!   [`CookieResolver`], [`SessionResolver`], and [`AcceptLanguageResolver`].
//! - [`make_t_function`] — builds the MiniJinja `t()` function wired up by
//!   [`crate::template`].
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::i18n::{I18n, I18nConfig};
//!
//! # fn example() -> modo::Result<()> {
//! let i18n = I18n::new(&I18nConfig::default())?;
//! let router: axum::Router = axum::Router::new().layer(i18n.layer());
//! # let _ = router;
//! # Ok(())
//! # }
//! ```

mod config;
mod extractor;
mod factory;
mod layer;
mod locale;
mod store;

pub use config::I18nConfig;
pub use extractor::Translator;
pub use factory::I18n;
pub use layer::I18nLayer;
pub use locale::{
    AcceptLanguageResolver, CookieResolver, LocaleResolver, QueryParamResolver, SessionResolver,
};
pub use store::{TranslationStore, make_t_function};
