pub mod config;
pub mod manager;

pub use config::{CookieConfig, CookieOptions, SameSite};
pub(crate) use manager::build_cookie;
pub use manager::CookieManager;
