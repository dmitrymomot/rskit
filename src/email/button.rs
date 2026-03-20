/// Button type variants for email buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonType {
    Primary,
    Danger,
    Warning,
    Info,
    Success,
}

impl ButtonType {
    /// Returns (background_color, text_color) for this button type.
    pub fn colors<'a>(&self, brand_color: Option<&'a str>) -> (&'a str, &'a str) {
        match self {
            Self::Primary => (brand_color.unwrap_or("#2563eb"), "#ffffff"),
            Self::Danger => ("#dc2626", "#ffffff"),
            Self::Warning => ("#d97706", "#ffffff"),
            Self::Info => ("#0891b2", "#ffffff"),
            Self::Success => ("#16a34a", "#ffffff"),
        }
    }
}

/// Parse button text like "button|Label" or "button:type|Label".
/// Returns `Some((ButtonType, label))` if it matches, `None` otherwise.
pub fn parse_button(text: &str) -> Option<(ButtonType, &str)> {
    let rest = text.strip_prefix("button")?;

    if let Some(rest) = rest.strip_prefix('|') {
        // "button|Label" -> Primary
        if rest.is_empty() {
            return None;
        }
        return Some((ButtonType::Primary, rest));
    }

    if let Some(rest) = rest.strip_prefix(':') {
        // "button:type|Label"
        let (type_str, label) = rest.split_once('|')?;
        if label.is_empty() {
            return None;
        }
        let btn_type = match type_str {
            "primary" => ButtonType::Primary,
            "danger" => ButtonType::Danger,
            "warning" => ButtonType::Warning,
            "info" => ButtonType::Info,
            "success" => ButtonType::Success,
            _ => return None,
        };
        return Some((btn_type, label));
    }

    None
}

/// Render a table-based HTML button (Outlook-compatible).
pub fn render_button_html(
    label: &str,
    url: &str,
    btn_type: ButtonType,
    brand_color: Option<&str>,
) -> String {
    let (bg, fg) = btn_type.colors(brand_color);
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin: 16px 0;"><tr><td style="background-color: {bg}; border-radius: 6px; padding: 12px 24px;"><a href="{url}" style="color: {fg}; text-decoration: none; font-weight: 600; display: inline-block;">{label}</a></td></tr></table>"#
    )
}

/// Render a plain text button.
pub fn render_button_text(label: &str, url: &str) -> String {
    format!("{label}: {url}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_button_primary_default() {
        let (btn_type, label) = parse_button("button|Get Started").unwrap();
        assert_eq!(btn_type, ButtonType::Primary);
        assert_eq!(label, "Get Started");
    }

    #[test]
    fn parse_button_with_type() {
        let (btn_type, label) = parse_button("button:danger|Delete Account").unwrap();
        assert_eq!(btn_type, ButtonType::Danger);
        assert_eq!(label, "Delete Account");
    }

    #[test]
    fn parse_button_all_types() {
        assert_eq!(
            parse_button("button:primary|X").unwrap().0,
            ButtonType::Primary
        );
        assert_eq!(
            parse_button("button:danger|X").unwrap().0,
            ButtonType::Danger
        );
        assert_eq!(
            parse_button("button:warning|X").unwrap().0,
            ButtonType::Warning
        );
        assert_eq!(parse_button("button:info|X").unwrap().0, ButtonType::Info);
        assert_eq!(
            parse_button("button:success|X").unwrap().0,
            ButtonType::Success
        );
    }

    #[test]
    fn parse_button_not_a_button() {
        assert!(parse_button("Click here").is_none());
        assert!(parse_button("").is_none());
        assert!(parse_button("button").is_none());
        assert!(parse_button("button|").is_none());
        assert!(parse_button("button:unknown|Label").is_none());
        assert!(parse_button("button:danger|").is_none());
    }

    #[test]
    fn render_html_contains_expected_parts() {
        let html = render_button_html("Go", "https://x.com", ButtonType::Primary, None);
        assert!(html.contains("background-color: #2563eb"));
        assert!(html.contains("href=\"https://x.com\""));
        assert!(html.contains(">Go</a>"));
        assert!(html.contains("role=\"presentation\""));
    }

    #[test]
    fn render_html_brand_color_overrides_primary() {
        let html = render_button_html("Go", "https://x.com", ButtonType::Primary, Some("#ff0000"));
        assert!(html.contains("background-color: #ff0000"));
    }

    #[test]
    fn render_html_brand_color_does_not_affect_other_types() {
        let html = render_button_html("Go", "https://x.com", ButtonType::Danger, Some("#ff0000"));
        assert!(html.contains("background-color: #dc2626"));
    }

    #[test]
    fn render_text_format() {
        let text = render_button_text("Get Started", "https://example.com");
        assert_eq!(text, "Get Started: https://example.com");
    }
}
