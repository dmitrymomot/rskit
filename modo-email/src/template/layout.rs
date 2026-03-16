use std::path::Path;

pub(crate) const DEFAULT_LAYOUT: &str = r#"<!DOCTYPE html>
<html lang="en" xmlns:v="urn:schemas-microsoft-com:vml">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<title>{{subject}}</title>
<style>
  @media (prefers-color-scheme: dark) {
    body { background-color: #1a1a1a !important; }
    .email-wrapper { background-color: #2d2d2d !important; }
    .email-body { color: #e0e0e0 !important; }
  }
  @media only screen and (max-width: 620px) {
    .email-wrapper { width: 100% !important; padding: 16px !important; }
  }
</style>
</head>
<body style="margin:0;padding:0;background-color:#f4f4f5;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color:#f4f4f5">
<tr><td align="center" style="padding:32px 16px">
  <!--[if mso]><table role="presentation" width="600" cellpadding="0" cellspacing="0"><tr><td><![endif]-->
  <table role="presentation" class="email-wrapper" cellpadding="0" cellspacing="0" style="max-width:600px;width:100%;background-color:#ffffff;border-radius:8px;overflow:hidden">
    {% if logo_url %}
    <tr><td style="padding:24px 32px 0;text-align:center">
      <img src="{{logo_url}}" alt="{{product_name | default(value="")}}" style="max-height:48px;width:auto">
    </td></tr>
    {% endif %}
    <tr><td class="email-body" style="padding:32px;color:#1f2937;font-size:16px;line-height:1.6">
      {{content}}
    </td></tr>
    <tr><td style="padding:16px 32px 32px;color:#6b7280;font-size:13px;text-align:center;border-top:1px solid #e5e7eb">
      {{footer_text | default(value="")}}
    </td></tr>
  </table>
  <!--[if mso]></td></tr></table><![endif]-->
</td></tr>
</table>
</body>
</html>"#;

/// Renders HTML layout templates using [MiniJinja](https://docs.rs/minijinja).
///
/// The engine always includes a built-in `"default"` layout. Additional layouts
/// are loaded from `{templates_path}/layouts/*.html` at construction time and
/// override the built-in if they share the same name.
///
/// Auto-escaping is disabled because the `content` variable is already rendered HTML.
pub struct LayoutEngine {
    env: minijinja::Environment<'static>,
}

impl LayoutEngine {
    /// Create a `LayoutEngine` that loads custom `.html` layouts from
    /// `{templates_path}/layouts/` in addition to the built-in `"default"` layout.
    ///
    /// Returns an error if any layout file contains invalid template syntax.
    pub fn try_new(templates_path: &str) -> Result<Self, modo::Error> {
        let mut env = Self::base_env();

        let layouts_dir = Path::new(templates_path).join("layouts");
        if layouts_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&layouts_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "html")
                    && let (Some(stem), Ok(content)) = (
                        path.file_stem().and_then(|s| s.to_str()),
                        std::fs::read_to_string(&path),
                    )
                {
                    env.add_template_owned(format!("layouts/{stem}.html"), content)
                        .map_err(|e| {
                            modo::Error::internal(format!(
                                "invalid layout template '{stem}.html': {e}"
                            ))
                        })?;
                }
            }
        }

        Ok(Self { env })
    }

    /// Create a `LayoutEngine` that loads custom `.html` layouts from
    /// `{templates_path}/layouts/` in addition to the built-in `"default"` layout.
    ///
    /// # Panics
    /// Panics if any layout file contains invalid template syntax.
    /// Use [`try_new`](Self::try_new) for a fallible alternative.
    pub fn new(templates_path: &str) -> Self {
        Self::try_new(templates_path).expect("all layout templates must be valid")
    }

    /// Create a `LayoutEngine` with only the built-in `"default"` layout.
    ///
    /// Useful in tests or when no custom layouts are needed.
    pub fn builtin_only() -> Self {
        Self {
            env: Self::base_env(),
        }
    }

    /// Render the named layout with the provided MiniJinja context.
    ///
    /// `layout_name` is looked up as `layouts/{layout_name}.html`. Returns an
    /// error if the layout does not exist or if rendering fails.
    pub fn render(
        &self,
        layout_name: &str,
        context: &minijinja::Value,
    ) -> Result<String, modo::Error> {
        let template_name = format!("layouts/{layout_name}.html");

        let tmpl = self.env.get_template(&template_name).map_err(|_| {
            tracing::debug!(layout_name = %layout_name, "email layout not found");
            modo::Error::internal(format!("Layout not found: {layout_name}"))
        })?;

        tmpl.render(context).map_err(|e| {
            tracing::error!(layout_name = %layout_name, error = %e, "email layout render failed");
            modo::Error::internal(format!("Layout render error: {e}"))
        })
    }

    /// Creates a base environment with the built-in default layout and
    /// auto-escaping disabled (email content is pre-rendered HTML).
    fn base_env() -> minijinja::Environment<'static> {
        let mut env = minijinja::Environment::new();
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
        env.add_template_owned(
            "layouts/default.html".to_string(),
            DEFAULT_LAYOUT.to_string(),
        )
        .expect("built-in layout is valid");
        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn render_with_builtin_layout() {
        let engine = LayoutEngine::builtin_only();
        let ctx = minijinja::context! {
            content => "<p>Hello</p>",
            subject => "Test",
        };

        let html = engine.render("default", &ctx).unwrap();
        assert!(html.contains("<p>Hello</p>"));
        assert!(html.contains("Test")); // subject in <title>
        assert!(html.contains("max-width")); // responsive wrapper
    }

    #[test]
    fn custom_layout_overrides_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let layouts_dir = dir.path().join("layouts");
        fs::create_dir_all(&layouts_dir).unwrap();
        fs::write(
            layouts_dir.join("default.html"),
            "<html><body>CUSTOM: {{content}}</body></html>",
        )
        .unwrap();

        let engine = LayoutEngine::new(dir.path().to_str().unwrap());
        let ctx = minijinja::context! {
            content => "<p>Hi</p>",
        };

        let html = engine.render("default", &ctx).unwrap();
        assert!(html.contains("CUSTOM: <p>Hi</p>"));
    }

    #[test]
    fn missing_layout_errors() {
        let engine = LayoutEngine::builtin_only();
        let ctx = minijinja::context! {};
        let result = engine.render("nonexistent", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn empty_layout_name() {
        let engine = LayoutEngine::builtin_only();
        let ctx = minijinja::context! {};
        let result = engine.render("", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn missing_optional_context_vars() {
        let engine = LayoutEngine::builtin_only();
        let ctx = minijinja::context! {
            content => "<p>Hello</p>",
            subject => "Test",
        };
        // No logo_url or footer_text — should render without error
        let html = engine.render("default", &ctx).unwrap();
        assert!(html.contains("<p>Hello</p>"));
        assert!(html.contains("Test"));
        // logo_url block should be skipped ({% if logo_url %} is falsy)
        assert!(!html.contains("<img"));
    }

    #[test]
    fn context_with_html_in_content() {
        let engine = LayoutEngine::builtin_only();
        let ctx = minijinja::context! {
            content => "<h1>Title</h1><p>Body &amp; more</p>",
            subject => "Test",
        };
        let html = engine.render("default", &ctx).unwrap();
        // Auto-escape is disabled, so HTML should be rendered verbatim
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<p>Body &amp; more</p>"));
    }

    #[test]
    fn invalid_layout_syntax_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let layouts_dir = dir.path().join("layouts");
        fs::create_dir_all(&layouts_dir).unwrap();
        fs::write(layouts_dir.join("broken.html"), "{% if unclosed %}").unwrap();

        let result = LayoutEngine::try_new(dir.path().to_str().unwrap());
        assert!(result.is_err());
    }
}
