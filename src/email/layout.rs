use crate::{Error, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::email::render;

/// Built-in responsive table-based base email layout.
///
/// Shell is XHTML 1.0 Transitional (for Outlook's Word rendering engine).
/// Light-theme colours are inline on every structural element so clients
/// that strip `<style>` (Gmail mobile webmail) still render correctly.
/// The retained `<style>` block provides:
/// - Dark-mode overrides via `@media (prefers-color-scheme: dark)` for
///   clients that honour it (Apple Mail, desktop Gmail).
/// - Mobile padding overrides via `@media only screen and (max-width: 620px)`.
/// - MSO / WebKit text-size-adjust resets.
///
/// Card max-width is 600px. Content width is fluid below that.
pub const BASE_LAYOUT: &str = r##"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" lang="en">
<head>
<meta http-equiv="Content-Type" content="text/html; charset=UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<meta http-equiv="X-UA-Compatible" content="IE=edge" />
<meta name="color-scheme" content="light dark" />
<meta name="supported-color-schemes" content="light dark" />
<style type="text/css">
body, table, td, a { -webkit-text-size-adjust: 100%; -ms-text-size-adjust: 100%; }
table, td { mso-table-lspace: 0pt; mso-table-rspace: 0pt; }
img { -ms-interpolation-mode: bicubic; max-width: 100%; }
body { margin: 0 !important; padding: 0 !important; width: 100% !important; }
@media (prefers-color-scheme: dark) {
  .email-body { background-color: #1a1a1a !important; }
  .email-card { background-color: #1a1a1a !important; }
  .email-content, .email-content * { color: #e4e4e7 !important; }
  .email-footer { color: #a1a1aa !important; }
  .email-divider { border-color: #3f3f46 !important; }
  .email-otp-bg { background-color: #27272a !important; color: #e4e4e7 !important; }
}
@media only screen and (max-width: 620px) {
  .email-outer { padding: 16px 8px !important; }
  .email-card { padding: 24px 16px !important; }
}
</style>
</head>
<body class="email-body" style="margin:0;padding:0;width:100%;background-color:#ffffff;-webkit-font-smoothing:antialiased;">
<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" class="email-body" style="background-color:#ffffff;">
<tr>
<td class="email-outer" align="center" style="padding:24px 16px;">
<!--[if mso]><table role="presentation" width="600" cellpadding="0" cellspacing="0"><tr><td><![endif]-->
<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" style="width:100%;max-width:600px;">
<tr>
<td class="email-card email-content" style="background-color:#ffffff;padding:32px;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;font-size:16px;line-height:1.6;color:#18181b;">
<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%">
{{logo_section}}
<tr><td>{{content}}</td></tr>
{{footer_section}}
</table>
</td>
</tr>
</table>
<!--[if mso]></td></tr></table><![endif]-->
</td>
</tr>
</table>
</body>
</html>"##;

/// Logo row when `logo_url` is present but `app_url` is not — bare `<img>`.
const LOGO_SECTION_BARE: &str = r#"<tr><td align="left" style="padding-bottom:24px;"><img src="{{logo_url}}" alt="" style="max-width:96px;max-height:48px;height:auto;width:auto;display:block;border:0;" /></td></tr>"#;

/// Logo row when both `logo_url` and `app_url` are present — linked `<img>`.
const LOGO_SECTION_LINKED: &str = r#"<tr><td align="left" style="padding-bottom:24px;"><a href="{{app_url}}" style="text-decoration:none;border:0;"><img src="{{logo_url}}" alt="" style="max-width:96px;max-height:48px;height:auto;width:auto;display:block;border:0;" /></a></td></tr>"#;

const FOOTER_SECTION: &str = concat!(
    r#"<tr><td style="padding-top:40px;font-size:0;line-height:0;">&nbsp;</td></tr>"#,
    r#"<tr><td class="email-footer email-divider" align="left" style="border-top:1px solid #e4e4e7;padding-top:8px;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;font-size:13px;color:#71717a;">{{footer_text}}</td></tr>"#,
);

/// Load custom layouts from the given directory.
///
/// Returns a map of layout name → layout HTML content, keyed by the file
/// stem (e.g., `"marketing"` for `marketing.html`). Non-HTML files are
/// silently ignored. If `layouts_path` does not exist, an empty map is
/// returned without error.
///
/// # Errors
///
/// Returns an error if the directory exists but cannot be read, or if any
/// `.html` file inside it cannot be read.
pub fn load_layouts(layouts_path: &str) -> Result<HashMap<String, String>> {
    let path = Path::new(layouts_path);
    let mut layouts = HashMap::new();

    if !path.exists() {
        return Ok(layouts);
    }

    let entries = std::fs::read_dir(path)
        .map_err(|e| Error::internal(format!("failed to read layouts directory: {e}")))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| Error::internal(format!("failed to read layout entry: {e}")))?;
        let file_path = entry.path();

        if file_path.extension().and_then(|e| e.to_str()) == Some("html")
            && let Some(name) = file_path.file_stem().and_then(|s| s.to_str())
        {
            let content = std::fs::read_to_string(&file_path).map_err(|e| {
                Error::internal(format!(
                    "failed to read layout '{}': {e}",
                    file_path.display()
                ))
            })?;
            layouts.insert(name.to_string(), content);
        }
    }

    Ok(layouts)
}

/// Apply a layout to rendered HTML content.
///
/// Resolves the special `{{content}}` placeholder with the rendered body,
/// conditionally injects `{{logo_section}}` (when `logo_url` is in `vars`)
/// and `{{footer_section}}` (when `footer_text` is in `vars`), then performs
/// full variable substitution over the combined HTML.
pub fn apply_layout(layout_html: &str, content: &str, vars: &HashMap<String, String>) -> String {
    let logo_section = if vars.contains_key("logo_url") {
        let tmpl = if vars.contains_key("app_url") {
            LOGO_SECTION_LINKED
        } else {
            LOGO_SECTION_BARE
        };
        render::substitute(tmpl, vars)
    } else {
        String::new()
    };

    let footer_section = if vars.contains_key("footer_text") {
        render::substitute(FOOTER_SECTION, vars)
    } else {
        String::new()
    };

    let mut full_vars = vars.clone();
    full_vars.insert("content".into(), content.into());
    full_vars.insert("logo_section".into(), logo_section);
    full_vars.insert("footer_section".into(), footer_section);

    render::substitute(layout_html, &full_vars)
}

/// Resolve a layout name to its HTML content.
///
/// `"base"` returns the built-in responsive layout ([`BASE_LAYOUT`]); any
/// other name is looked up in the `custom_layouts` map loaded by
/// [`load_layouts`].
///
/// # Errors
///
/// Returns a 404 error when `name` is not `"base"` and is not present in
/// `custom_layouts`.
pub fn resolve_layout<'a>(
    name: &str,
    custom_layouts: &'a HashMap<String, String>,
) -> Result<std::borrow::Cow<'a, str>> {
    if name == "base" {
        Ok(std::borrow::Cow::Borrowed(BASE_LAYOUT))
    } else {
        custom_layouts
            .get(name)
            .map(|s| std::borrow::Cow::Borrowed(s.as_str()))
            .ok_or_else(|| Error::not_found(format!("email layout '{name}' not found")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_layout_has_content_placeholder() {
        assert!(BASE_LAYOUT.contains("{{content}}"));
    }

    #[test]
    fn base_layout_has_dark_mode() {
        assert!(BASE_LAYOUT.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn base_layout_has_max_width() {
        assert!(BASE_LAYOUT.contains("max-width:600px"));
    }

    #[test]
    fn apply_layout_injects_content() {
        let layout = "<div>{{content}}</div>";
        let result = apply_layout(layout, "<p>Hello</p>", &HashMap::new());
        assert_eq!(result, "<div><p>Hello</p></div>");
    }

    #[test]
    fn apply_layout_substitutes_vars() {
        let layout = "<div style=\"color: {{brand_color}}\">{{content}}</div>";
        let mut vars = HashMap::new();
        vars.insert("brand_color".into(), "#ff0000".into());
        let result = apply_layout(layout, "Body", &vars);
        assert!(result.contains("color: #ff0000"));
    }

    #[test]
    fn apply_layout_logo_section_when_var_present() {
        let mut vars = HashMap::new();
        vars.insert("logo_url".into(), "https://example.com/logo.png".into());
        let result = apply_layout(BASE_LAYOUT, "<p>Hello</p>", &vars);
        assert!(result.contains("https://example.com/logo.png"));
        assert!(result.contains("<img"));
    }

    #[test]
    fn apply_layout_no_logo_when_var_absent() {
        let result = apply_layout(BASE_LAYOUT, "<p>Hello</p>", &HashMap::new());
        assert!(!result.contains("<img"));
    }

    #[test]
    fn apply_layout_footer_section_when_var_present() {
        let mut vars = HashMap::new();
        vars.insert("footer_text".into(), "Copyright 2026".into());
        let result = apply_layout(BASE_LAYOUT, "<p>Hello</p>", &vars);
        assert!(result.contains("Copyright 2026"));
    }

    #[test]
    fn apply_layout_no_footer_when_var_absent() {
        let result = apply_layout(BASE_LAYOUT, "<p>Hello</p>", &HashMap::new());
        // The CSS rule .email-footer is always in <style>, but the actual
        // <td class="email-footer"> element should not be rendered
        assert!(!result.contains(r#"class="email-footer""#));
    }

    #[test]
    fn resolve_layout_base() {
        let customs = HashMap::new();
        let layout = resolve_layout("base", &customs).unwrap();
        assert!(layout.contains("{{content}}"));
    }

    #[test]
    fn resolve_layout_custom_found() {
        let mut customs = HashMap::new();
        customs.insert("marketing".into(), "<html>{{content}}</html>".into());
        let layout = resolve_layout("marketing", &customs).unwrap();
        assert_eq!(layout.as_ref(), "<html>{{content}}</html>");
    }

    #[test]
    fn resolve_layout_custom_not_found() {
        let customs = HashMap::new();
        let result = resolve_layout("missing", &customs);
        assert!(result.is_err());
    }

    #[test]
    fn load_layouts_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let layouts = load_layouts(dir.path().to_str().unwrap()).unwrap();
        assert!(layouts.is_empty());
    }

    #[test]
    fn load_layouts_reads_html_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("custom.html"), "<div>{{content}}</div>").unwrap();
        std::fs::write(dir.path().join("ignore.txt"), "not a layout").unwrap();
        let layouts = load_layouts(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(layouts.len(), 1);
        assert!(layouts.contains_key("custom"));
        assert_eq!(layouts["custom"], "<div>{{content}}</div>");
    }

    #[test]
    fn load_layouts_nonexistent_dir_returns_empty() {
        let layouts = load_layouts("/nonexistent/path/that/does/not/exist").unwrap();
        assert!(layouts.is_empty());
    }

    #[test]
    fn base_layout_is_xhtml_transitional() {
        assert!(BASE_LAYOUT.contains("-//W3C//DTD XHTML 1.0 Transitional"));
    }

    #[test]
    fn base_layout_has_table_shell() {
        // Outer presentation table wraps the content
        let count = BASE_LAYOUT.matches(r#"role="presentation""#).count();
        assert!(count >= 2, "expected ≥2 presentation tables in base layout");
    }

    #[test]
    fn base_layout_has_inline_light_styles() {
        // Body background inline on <body> — flat white in light mode
        assert!(
            BASE_LAYOUT.contains(r#"background-color:#ffffff"#)
                || BASE_LAYOUT.contains(r#"background-color: #ffffff"#)
        );
    }

    #[test]
    fn base_layout_has_otp_dark_override() {
        // OTP pill gets dark-mode override via .email-otp-bg class
        assert!(BASE_LAYOUT.contains(".email-otp-bg"));
    }

    #[test]
    fn apply_layout_logo_is_left_aligned() {
        let mut vars = HashMap::new();
        vars.insert("logo_url".into(), "https://cdn.example.com/logo.png".into());
        let result = apply_layout(BASE_LAYOUT, "<p>x</p>", &vars);
        assert!(result.contains(r#"align="left""#));
        assert!(result.contains("max-width:96px") || result.contains("max-width: 96px"));
    }

    #[test]
    fn apply_layout_footer_has_divider() {
        let mut vars = HashMap::new();
        vars.insert("footer_text".into(), "© 2026".into());
        let result = apply_layout(BASE_LAYOUT, "<p>x</p>", &vars);
        assert!(result.contains("email-divider"));
        assert!(result.contains("border-top:1px solid"));
    }

    #[test]
    fn base_layout_keeps_dark_mode_in_style() {
        assert!(BASE_LAYOUT.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn base_layout_keeps_mobile_media_query() {
        assert!(BASE_LAYOUT.contains("max-width: 620px"));
    }

    #[test]
    fn apply_layout_logo_wraps_in_link_when_app_url_present() {
        let mut vars = HashMap::new();
        vars.insert("logo_url".into(), "https://cdn.example.com/logo.png".into());
        vars.insert("app_url".into(), "https://example.com".into());
        let result = apply_layout(BASE_LAYOUT, "<p>x</p>", &vars);
        assert!(result.contains("<a href=\"https://example.com\""));
        assert!(result.contains("https://cdn.example.com/logo.png"));
    }

    #[test]
    fn apply_layout_logo_bare_when_only_logo_url_present() {
        let mut vars = HashMap::new();
        vars.insert("logo_url".into(), "https://cdn.example.com/logo.png".into());
        let result = apply_layout(BASE_LAYOUT, "<p>x</p>", &vars);
        assert!(result.contains("https://cdn.example.com/logo.png"));
        assert!(!result.contains("<a href=\"https://example.com\""));
        assert!(!result.contains("{{app_url}}"));
    }

    #[test]
    fn apply_layout_no_logo_without_logo_url() {
        let result = apply_layout(BASE_LAYOUT, "<p>x</p>", &HashMap::new());
        assert!(!result.contains("<img"));
    }
}
