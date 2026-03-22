mod functions;
mod html;
mod traits;

pub use functions::{
    collapse_whitespace, normalize_email, strip_html, trim, trim_lowercase, truncate,
};
pub use traits::Sanitize;
