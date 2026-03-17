//! Markdown rendering for email bodies.
//!
//! Converts Markdown to HTML and plain text in a single pass. The custom
//! `[button|Label](url)` link syntax is converted to email-safe table-based
//! CTA buttons in HTML and to `Label (url)` in plain text.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

const BUTTON_PREFIX: &str = "button|";

/// Default button background color used when no `brand_color` is set (`#4F46E5`, indigo).
pub const DEFAULT_BUTTON_COLOR: &str = "#4F46E5";

/// Render Markdown to both HTML and plain text in a single pass.
///
/// `[button|Label](url)` links become email-safe button tables in HTML
/// and `Label (url)` in plain text.
pub fn render(markdown: &str, button_color: &str) -> (String, String) {
    let parser = Parser::new_ext(markdown, Options::empty());

    let mut html = String::new();
    let mut text = String::new();
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
            Event::Text(ref t) if in_link => {
                link_text.push_str(t);
            }
            Event::End(TagEnd::Link) if in_link => {
                in_link = false;
                if let Some(label) = link_text.strip_prefix(BUTTON_PREFIX) {
                    html.push_str(&render_button(label, &link_url, button_color));
                    text.push_str(&format!("{label} ({link_url})"));
                } else {
                    html.push_str(&format!(
                        "<a href=\"{}\" style=\"color:{button_color}\">{}</a>",
                        link_url, link_text,
                    ));
                    text.push_str(&format!("{link_text} ({link_url})"));
                }
            }
            _ if in_link => {}
            _ => {
                match &event {
                    Event::Text(t) => text.push_str(t),
                    Event::SoftBreak | Event::HardBreak => text.push('\n'),
                    Event::End(TagEnd::Paragraph) => text.push_str("\n\n"),
                    Event::End(TagEnd::Heading(_)) => text.push_str("\n\n"),
                    _ => {}
                }
                pulldown_cmark::html::push_html(&mut html, std::iter::once(event));
            }
        }
    }

    (html, text.trim().to_string())
}

/// Render Markdown to HTML, converting `[button|Label](url)` into email-safe button tables.
pub fn render_markdown(markdown: &str) -> String {
    render(markdown, DEFAULT_BUTTON_COLOR).0
}

/// Render Markdown to HTML with a custom button background color.
pub fn render_markdown_with_color(markdown: &str, button_color: &str) -> String {
    render(markdown, button_color).0
}

/// Convert Markdown to plain text, stripping formatting and converting links to `Text (URL)` form.
pub fn render_plain_text(markdown: &str) -> String {
    render(markdown, DEFAULT_BUTTON_COLOR).1
}

fn render_button(label: &str, url: &str, color: &str) -> String {
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:16px auto"><tr><td style="background-color:{color};border-radius:6px;padding:12px 24px;text-align:center"><a href="{url}" style="color:#ffffff;text-decoration:none;font-weight:bold;font-size:16px;display:inline-block">{label}</a></td></tr></table>"#,
    )
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

    #[test]
    fn combined_render_produces_both() {
        let (html, text) = render("Hi **world**\n\n[button|Click](https://url.com)", "#ff0000");
        assert!(html.contains("<strong>world</strong>"));
        assert!(html.contains("role=\"presentation\""));
        assert!(html.contains("#ff0000"));
        assert!(text.contains("Hello world") || text.contains("Hi world"));
        assert!(text.contains("Click (https://url.com)"));
    }

    #[test]
    fn empty_markdown() {
        let (html, text) = render("", DEFAULT_BUTTON_COLOR);
        assert!(html.is_empty());
        assert!(text.is_empty());
    }

    #[test]
    fn plain_text_no_formatting() {
        let (html, text) = render("Just text", DEFAULT_BUTTON_COLOR);
        assert!(html.contains("<p>Just text</p>"));
        assert_eq!(text, "Just text");
    }

    #[test]
    fn multiple_buttons() {
        let md = "[button|First](https://a.com)\n\n[button|Second](https://b.com)";
        let (html, text) = render(md, DEFAULT_BUTTON_COLOR);
        // Two button tables in HTML
        assert_eq!(html.matches("role=\"presentation\"").count(), 2);
        assert!(html.contains("First"));
        assert!(html.contains("Second"));
        // Two labeled links in text
        assert!(text.contains("First (https://a.com)"));
        assert!(text.contains("Second (https://b.com)"));
    }

    #[test]
    fn empty_button_label() {
        let md = "[button|](https://example.com)";
        let (html, text) = render(md, DEFAULT_BUTTON_COLOR);
        assert!(html.contains("role=\"presentation\""));
        assert!(html.contains("https://example.com"));
        assert!(text.contains("(https://example.com)"));
    }

    #[test]
    fn heading_rendering() {
        let (html, text) = render("# Title", DEFAULT_BUTTON_COLOR);
        assert!(html.contains("<h1>"));
        assert!(html.contains("Title"));
        assert!(text.contains("Title"));
    }

    #[test]
    fn list_rendering() {
        let md = "- item1\n- item2";
        let html = render_markdown(md);
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>item1</li>"));
        assert!(html.contains("<li>item2</li>"));
    }

    #[test]
    fn code_block_preserved() {
        let md = "```\nfn main() {}\n```";
        let html = render_markdown(md);
        assert!(html.contains("<pre><code>"));
        assert!(html.contains("fn main() {}"));
    }

    #[test]
    fn html_entities_escaped_in_text() {
        let md = "Use &amp; and <tag>";
        let text = render_plain_text(md);
        assert!(text.contains("&") || text.contains("&amp;"));
    }
}
