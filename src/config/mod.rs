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
//! ## Provides
//!
//! - [`Config`] — top-level framework configuration struct with feature-gated
//!   fields for every built-in module.
//! - [`load::<T>(dir)`](load) — reads `{dir}/{APP_ENV}.yaml`, substitutes env
//!   vars, and deserializes into `T`.
//! - [`env()`](env) — returns the current `APP_ENV` value (default: `"development"`).
//! - [`is_dev()`](is_dev), [`is_prod()`](is_prod), [`is_test()`](is_test) —
//!   environment predicates.
//! - [`substitute`] — submodule exposing [`substitute::substitute_env_vars`] for
//!   replacing `${VAR}` placeholders in arbitrary strings.
//!
//! ## Quick start
//!
//! ```no_run
//! use modo::config::load;
//! use modo::Config;
//!
//! // Reads config/development.yaml (or whatever APP_ENV resolves to)
//! let config: Config = load("config/").unwrap();
//! ```

mod env;
mod load;
mod modo;
pub mod substitute;

pub use env::{env, is_dev, is_prod, is_test};
pub use load::load;
pub use modo::Config;
