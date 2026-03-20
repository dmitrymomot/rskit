mod config;
mod context;
mod htmx;
#[allow(dead_code)]
mod i18n;
mod locale;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use htmx::HxRequest;
pub use locale::{
    AcceptLanguageResolver, CookieResolver, LocaleResolver, QueryParamResolver, SessionResolver,
};
