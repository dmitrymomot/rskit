use minijinja::{Environment, Error, ErrorKind, State};

/// Register CSRF template functions on the MiniJinja environment.
///
/// Registers:
/// - `csrf_field()` — returns `<input type="hidden" ...>` HTML for forms
/// - `csrf_token()` — returns the raw token string (for meta tags / JS)
///
/// Both read `csrf_token` from the template render context, injected by
/// the CSRF middleware via `TemplateContext`.
pub fn register_template_functions(env: &mut Environment<'static>) {
    env.add_function("csrf_field", |state: &State| -> Result<String, Error> {
        let token = state
            .lookup("csrf_token")
            .map(|v: minijinja::Value| v.to_string())
            .unwrap_or_default();

        if token.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidOperation,
                "csrf_token not found in template context — is the CSRF middleware active?",
            ));
        }

        Ok(format!(
            r#"<input type="hidden" name="_csrf_token" value="{token}">"#
        ))
    });

    env.add_function("csrf_token", |state: &State| -> Result<String, Error> {
        let token = state
            .lookup("csrf_token")
            .map(|v: minijinja::Value| v.to_string())
            .unwrap_or_default();

        if token.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidOperation,
                "csrf_token not found in template context — is the CSRF middleware active?",
            ));
        }

        Ok(token)
    });
}
