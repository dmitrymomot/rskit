//! # modo::config
//!
//! YAML configuration loader with environment-variable substitution.
//!
//! Config files live in a directory (e.g. `config/`) and are named after the
//! active environment: `development.yaml`, `production.yaml`, `test.yaml`.
//! The active environment is read from the `APP_ENV` environment variable;
//! it defaults to `"development"` when unset.
//!
//! Before deserialization, `${VAR}` placeholders are replaced with values from
//! the process environment. Use `${VAR:default}` to supply a fallback when
//! `VAR` is not set.
//!
//! Provides:
//! - [`Config`] — top-level framework configuration struct composing the
//!   sub-config of every built-in module.
//! - [`load`] — reads `{dir}/{APP_ENV}.yaml`, substitutes env vars, and
//!   deserializes into `T`.
//! - [`env`] — returns the current `APP_ENV` value (default: `"development"`).
//! - [`is_dev`], [`is_prod`], [`is_test`] — environment predicates.
//! - [`substitute`] — submodule exposing [`substitute::substitute_env_vars`] for
//!   replacing `${VAR}` placeholders in arbitrary strings.
//!
//! ## Quick start
//!
//! ```no_run
//! use modo::Config;
//!
//! // Reads config/development.yaml (or whatever APP_ENV resolves to).
//! // Set `APP_ENV=production` to load config/production.yaml instead.
//! let config: Config = modo::config::load("config/").unwrap();
//! ```
//!
//! Note that `trusted_proxies` is a top-level field on [`Config`], not nested
//! under `server`. Time-based settings use `_secs: u64` fields — for example
//! `session.session_ttl_secs`, `session.touch_interval_secs`, and
//! `server.shutdown_timeout_secs`.

mod env;
mod load;
mod modo;
pub mod substitute;

pub use env::{env, is_dev, is_prod, is_test};
pub use load::load;
pub use modo::Config;
