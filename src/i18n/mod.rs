//! # modo::i18n
//!
//! Internationalization primitives for modo.
//!
//! Provides a [`TranslationStore`] loaded from YAML files on disk, a pluggable
//! [`LocaleResolver`] chain, a Tower [`I18nLayer`] that injects a per-request
//! [`Translator`] into request extensions, and an [`I18n`] factory that ties
//! the pieces together.
//!
//! The same [`TranslationStore`] powers the MiniJinja `t()` function registered
//! by [`modo::template`](crate::template) via
//! [`make_t_function`](self::make_t_function).
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

pub(crate) mod locale;

mod config;
mod extractor;
mod factory;
mod layer;
mod store;

pub use config::I18nConfig;
pub use extractor::Translator;
pub use factory::I18n;
pub use layer::I18nLayer;
pub use locale::{
    AcceptLanguageResolver, CookieResolver, LocaleResolver, QueryParamResolver, SessionResolver,
};
pub use store::{TranslationStore, make_t_function};
