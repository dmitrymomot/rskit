use std::fmt;

#[derive(Debug)]
pub enum TemplateError {
    /// Template not found in the engine.
    NotFound { name: String },
    /// MiniJinja render error.
    Render { source: minijinja::Error },
    /// Engine not registered as a service.
    EngineNotRegistered,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { name } => write!(f, "template not found: {name}"),
            Self::Render { source } => write!(f, "template render error: {source}"),
            Self::EngineNotRegistered => write!(f, "TemplateEngine not registered as a service"),
        }
    }
}

impl std::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render { source } => Some(source),
            _ => None,
        }
    }
}

impl From<minijinja::Error> for TemplateError {
    fn from(err: minijinja::Error) -> Self {
        if err.kind() == minijinja::ErrorKind::TemplateNotFound {
            Self::NotFound {
                name: err.template_source().unwrap_or("unknown").to_string(),
            }
        } else {
            Self::Render { source: err }
        }
    }
}
