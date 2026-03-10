use std::fmt;

#[derive(Debug)]
pub enum TemplateError {
    /// Template not found in the engine.
    NotFound { name: String },
    /// MiniJinja render error.
    Render { source: minijinja::Error },
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { name } => write!(f, "template not found: {name}"),
            Self::Render { source } => write!(f, "template render error: {source}"),
        }
    }
}

impl std::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render { source } => Some(source),
            Self::NotFound { .. } => None,
        }
    }
}

impl From<minijinja::Error> for TemplateError {
    fn from(err: minijinja::Error) -> Self {
        if err.kind() == minijinja::ErrorKind::TemplateNotFound {
            Self::NotFound {
                name: extract_template_name(&err),
            }
        } else {
            Self::Render { source: err }
        }
    }
}

/// Extracts the template name from a TemplateNotFound error's detail.
/// MiniJinja's detail format: `template "NAME" does not exist`
fn extract_template_name(err: &minijinja::Error) -> String {
    if let Some(detail) = err.detail() {
        if let Some(start) = detail.find('"')
            && let Some(end) = detail[start + 1..].find('"')
        {
            return detail[start + 1..start + 1 + end].to_string();
        }
        return detail.to_string();
    }
    "unknown".to_string()
}
