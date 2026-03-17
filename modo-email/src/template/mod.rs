//! Email template loading, rendering, and caching.
//!
//! The central abstraction is [`TemplateProvider`], which supplies [`EmailTemplate`]
//! values by name and locale. The built-in [`filesystem::FilesystemProvider`] loads `.md` files
//! from disk with locale-based fallback. Wrap any provider in [`CachedTemplateProvider`]
//! to add LRU caching.
//!
//! Markdown rendering and `{{var}}` substitution are in the [`markdown`] and [`vars`]
//! submodules respectively. HTML layout rendering lives in [`layout`].

pub mod cached;
mod email_template;
pub mod filesystem;
pub mod layout;
pub mod markdown;
pub mod vars;

pub use cached::CachedTemplateProvider;
pub use email_template::{EmailTemplate, TemplateProvider};
