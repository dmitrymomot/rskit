pub mod config;
pub mod manager;

pub use config::{CookieConfig, CookieOptions, SameSite};
pub use manager::CookieManager;
pub(crate) use manager::build_cookie;
