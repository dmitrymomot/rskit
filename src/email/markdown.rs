use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};

use crate::email::button;

/// Convert markdown to HTML, intercepting button syntax in links.
///
/// Strategy: buffer all events between `Start(Link)` and `End(Link)`,
/// then check if the concatenated text matches button syntax.
/// If yes, emit a table-based button. If no, flush all buffered events
/// as a normal link through `push_html`.
pub fn markdown_to_html(markdown: &str, brand_color: Option<&str>) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut html = String::new();

    // Link buffering state
    let mut link_url: Option<String> = None;
    let mut link_title: Option<CowStr> = None;
    let mut link_events: Vec<Event> = Vec::new();

    for event in parser {
        if link_url.is_some() {
            match &event {
                Event::End(TagEnd::Link) => {
                    let url = link_url.take().expect("guarded by is_some check");
                    let title = link_title.take();

                    // Concatenate all text events to check for button syntax
                    let full_text: String = link_events
                        .iter()
                        .filter_map(|e| match e {
                            Event::Text(t) => Some(t.as_ref()),
                            Event::Code(t) => Some(t.as_ref()),
                            _ => None,
                        })
                        .collect();

                    if let Some((btn_type, label)) = button::parse_button(&full_text) {
                        // Emit table-based button
                        html.push_str(&button::render_button_html(
                            label,
                            &url,
                            btn_type,
                            brand_color,
                        ));
                    } else {
                        // Flush as normal link: re-wrap in Start(Link) + events + End(Link)
                        let start = Event::Start(Tag::Link {
                            link_type: pulldown_cmark::LinkType::Inline,
                            dest_url: CowStr::from(url),
                            title: title.unwrap_or(CowStr::from("")),
                            id: CowStr::from(""),
                        });
                        let end = Event::End(TagEnd::Link);
                        let full_events: Vec<Event> = std::iter::once(start)
                            .chain(link_events.drain(..))
                            .chain(std::iter::once(end))
                            .collect();
                        pulldown_cmark::html::push_html(&mut html, full_events.into_iter());
                    }

                    link_events.clear();
                }
                _ => {
                    link_events.push(event);
                }
            }
        } else {
            match event {
                Event::Start(Tag::Link {
                    dest_url, title, ..
                }) => {
                    link_url = Some(dest_url.to_string());
                    link_title = Some(title);
                    link_events.clear();
                }
                _ => {
                    pulldown_cmark::html::push_html(&mut html, std::iter::once(event));
                }
            }
        }
    }

    html
}

/// Convert markdown to plain text.
pub fn markdown_to_text(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut text = String::new();
    let mut in_link: Option<String> = None; // holds URL
    let mut link_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push('\n');
            }
            Event::End(TagEnd::Heading(_)) => {
                text.push('\n');
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = Some(dest_url.to_string());
                link_text.clear();
            }
            Event::Text(t) if in_link.is_some() => {
                link_text.push_str(&t);
            }
            Event::Code(t) if in_link.is_some() => {
                link_text.push_str(&t);
            }
            Event::End(TagEnd::Link) => {
                if let Some(url) = in_link.take() {
                    if let Some((_, label)) = button::parse_button(&link_text) {
                        text.push_str(&button::render_button_text(label, &url));
                    } else {
                        text.push_str(&format!("{link_text} ({url})"));
                    }
                    link_text.clear();
                }
            }
            Event::Start(Tag::Item) => {
                text.push_str("- ");
            }
            Event::End(TagEnd::Item) => {
                if !text.ends_with('\n') {
                    text.push('\n');
                }
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                text.push_str("\n\n");
            }
            Event::Text(t) => {
                text.push_str(&t);
            }
            Event::SoftBreak | Event::HardBreak => {
                text.push('\n');
            }
            Event::Code(t) => {
                text.push_str(&t);
            }
            _ => {}
        }
    }

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_basic_paragraph() {
        let html = markdown_to_html("Hello **world**!", None);
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn html_heading() {
        let html = markdown_to_html("# Title\n\nBody", None);
        assert!(html.contains("<h1>Title</h1>"));
    }

    #[test]
    fn html_link() {
        let html = markdown_to_html("[Click](https://example.com)", None);
        assert!(html.contains("<a href=\"https://example.com\">Click</a>"));
    }

    #[test]
    fn html_button_primary_default() {
        let html = markdown_to_html("[button|Get Started](https://example.com)", None);
        assert!(html.contains("role=\"presentation\""));
        assert!(html.contains("background-color: #2563eb"));
        assert!(html.contains(">Get Started</a>"));
        assert!(html.contains("href=\"https://example.com\""));
    }

    #[test]
    fn html_button_with_type() {
        let html = markdown_to_html("[button:danger|Delete](https://example.com)", None);
        assert!(html.contains("background-color: #dc2626"));
        assert!(html.contains(">Delete</a>"));
    }

    #[test]
    fn html_button_brand_color() {
        let html = markdown_to_html("[button|Click](https://example.com)", Some("#ff0000"));
        assert!(html.contains("background-color: #ff0000"));
    }

    #[test]
    fn html_malformed_button_renders_as_link() {
        let html = markdown_to_html("[button:unknown|Click](https://example.com)", None);
        assert!(html.contains("<a href="));
        assert!(!html.contains("role=\"presentation\""));
    }

    #[test]
    fn html_list() {
        let html = markdown_to_html("- Item 1\n- Item 2", None);
        assert!(html.contains("<li>"));
    }

    #[test]
    fn text_basic_paragraph() {
        let text = markdown_to_text("Hello **world**!");
        assert_eq!(text, "Hello world!");
    }

    #[test]
    fn text_link() {
        let text = markdown_to_text("[Click](https://example.com)");
        assert_eq!(text, "Click (https://example.com)");
    }

    #[test]
    fn text_button() {
        let text = markdown_to_text("[button:primary|Get Started](https://example.com)");
        assert_eq!(text, "Get Started: https://example.com");
    }

    #[test]
    fn text_heading() {
        let text = markdown_to_text("# Title\n\nBody");
        assert!(text.contains("Title"));
        assert!(text.contains("Body"));
    }

    #[test]
    fn text_list() {
        let text = markdown_to_text("- Item 1\n- Item 2");
        assert!(text.contains("- Item 1"));
        assert!(text.contains("- Item 2"));
    }

    #[test]
    fn text_code_inside_link() {
        let text = markdown_to_text("[`code`](https://example.com)");
        assert_eq!(text, "code (https://example.com)");
    }
}
