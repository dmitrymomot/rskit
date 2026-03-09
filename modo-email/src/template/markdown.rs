use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

const BUTTON_PREFIX: &str = "button|";
const DEFAULT_BUTTON_COLOR: &str = "#4F46E5";

/// Render Markdown to HTML, converting `[button|Label](url)` into email-safe button tables.
pub fn render_markdown(markdown: &str) -> String {
    render_markdown_with_color(markdown, DEFAULT_BUTTON_COLOR)
}

/// Render Markdown to HTML with a custom button background color.
pub fn render_markdown_with_color(markdown: &str, button_color: &str) -> String {
    let opts = Options::empty();
    let parser = Parser::new_ext(markdown, opts);

    let mut html = String::new();
    let mut in_link = false;
    let mut link_url = String::new();
    let mut link_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = true;
                link_url = dest_url.to_string();
                link_text.clear();
            }
            Event::Text(text) if in_link => {
                link_text.push_str(&text);
            }
            Event::End(TagEnd::Link) if in_link => {
                in_link = false;
                if let Some(label) = link_text.strip_prefix(BUTTON_PREFIX) {
                    html.push_str(&render_button(label, &link_url, button_color));
                } else {
                    html.push_str(&format!(
                        "<a href=\"{}\" style=\"color:{button_color}\">{}</a>",
                        link_url, link_text,
                    ));
                }
            }
            _ if in_link => {}
            _ => {
                pulldown_cmark::html::push_html(&mut html, std::iter::once(event));
            }
        }
    }

    html
}

fn render_button(label: &str, url: &str, color: &str) -> String {
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:16px auto"><tr><td style="background-color:{color};border-radius:6px;padding:12px 24px;text-align:center"><a href="{url}" style="color:#ffffff;text-decoration:none;font-weight:bold;font-size:16px;display:inline-block">{label}</a></td></tr></table>"#,
    )
}

/// Convert Markdown to plain text, stripping formatting and converting links to `Text (URL)` form.
pub fn render_plain_text(markdown: &str) -> String {
    let opts = Options::empty();
    let parser = Parser::new_ext(markdown, opts);

    let mut text = String::new();
    let mut in_link = false;
    let mut link_text = String::new();
    let mut link_url = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = true;
                link_url = dest_url.to_string();
                link_text.clear();
            }
            Event::Text(t) if in_link => {
                link_text.push_str(&t);
            }
            Event::End(TagEnd::Link) if in_link => {
                in_link = false;
                let display = link_text.strip_prefix(BUTTON_PREFIX).unwrap_or(&link_text);
                text.push_str(&format!("{display} ({link_url})"));
            }
            Event::Text(t) => text.push_str(&t),
            Event::SoftBreak | Event::HardBreak => text.push('\n'),
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => text.push_str("\n\n"),
            Event::Start(Tag::Heading { .. }) => {}
            Event::End(TagEnd::Heading(_)) => text.push_str("\n\n"),
            _ => {}
        }
    }

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_basic_markdown() {
        let html = render_markdown("Hello **world**");
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn render_link_as_link() {
        let html = render_markdown("[Click](https://example.com)");
        assert!(html.contains("<a"));
        assert!(html.contains("href=\"https://example.com\""));
        assert!(html.contains("Click"));
    }

    #[test]
    fn render_button_link() {
        let html = render_markdown("[button|Get Started](https://example.com)");
        assert!(html.contains("Get Started"));
        assert!(html.contains("https://example.com"));
        assert!(html.contains("role=\"presentation\""));
        assert!(!html.contains("button|"));
    }

    #[test]
    fn render_normal_link_with_pipe() {
        let html = render_markdown("[some|text](https://example.com)");
        // "some" is not a known element type, render as normal link
        assert!(html.contains("some|text"));
        assert!(html.contains("<a"));
    }

    #[test]
    fn plain_text_from_markdown() {
        let text = render_plain_text(
            "Hello **world**\n\n[button|Click](https://url.com)\n\n[Link](https://other.com)",
        );
        assert!(text.contains("Hello world"));
        assert!(text.contains("Click (https://url.com)"));
        assert!(text.contains("Link (https://other.com)"));
        assert!(!text.contains("button|"));
    }
}
