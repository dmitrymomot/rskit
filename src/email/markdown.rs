use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};

use crate::email::button;
use crate::email::otp;

/// State of the source scanner used by the OTP pre-pass.
#[derive(Copy, Clone)]
enum ScanCtx {
    /// Normal markdown text.
    Text,
    /// Inside a single- or multi-backtick code span; `ticks` is the run length.
    CodeSpan { ticks: usize },
    /// Inside a fenced code block; `fence` is the opening fence character (`\`` or `~`).
    Fence { fence_char: u8 },
}

/// Walk `src` and, outside code spans / fenced blocks / escapes, replace
/// `[otp|CODE]` with `replace(CODE)`.
///
/// Non-ASCII bytes are pushed as full UTF-8 chars to avoid corruption.
fn transform_otp<F>(src: &str, mut replace: F) -> String
where
    F: FnMut(&str) -> String,
{
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len() + 64);
    let mut i = 0;
    let mut ctx = ScanCtx::Text;
    let mut at_line_start = true;

    while i < bytes.len() {
        match ctx {
            ScanCtx::Text => {
                // Fenced block open: line-leading ``` or ~~~
                if at_line_start {
                    let line = &src[i..];
                    let trimmed = line.trim_start_matches(' ');
                    let ch = trimmed.as_bytes().first().copied().unwrap_or(0);
                    if ch == b'`' || ch == b'~' {
                        let run_len = trimmed.bytes().take_while(|&b| b == ch).count();
                        if run_len >= 3 {
                            let nl = line.find('\n').map_or(line.len(), |n| n + 1);
                            out.push_str(&line[..nl]);
                            i += nl;
                            at_line_start = true;
                            ctx = ScanCtx::Fence { fence_char: ch };
                            continue;
                        }
                    }
                }

                let b = bytes[i];

                // Backslash-escaped bracket: pass through literally
                if b == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
                    out.push('\\');
                    out.push('[');
                    i += 2;
                    at_line_start = false;
                    continue;
                }

                // Backtick opens a code span
                if b == b'`' {
                    let ticks = bytes[i..].iter().take_while(|&&c| c == b'`').count();
                    out.push_str(&src[i..i + ticks]);
                    i += ticks;
                    at_line_start = false;
                    ctx = ScanCtx::CodeSpan { ticks };
                    continue;
                }

                // OTP pattern
                if b == b'[' && src[i..].starts_with("[otp|") {
                    let rest = &src[i + 5..];
                    if let Some(end) = rest.find(']') {
                        let code = &rest[..end];
                        if otp::is_valid_code(code) {
                            out.push_str(&replace(code));
                            i += 5 + end + 1;
                            at_line_start = false;
                            continue;
                        }
                    }
                }

                // Push character correctly: ASCII fast path, UTF-8 for non-ASCII
                if b.is_ascii() {
                    out.push(b as char);
                    at_line_start = b == b'\n';
                    i += 1;
                } else {
                    // Decode the full UTF-8 char at position i
                    let ch = src[i..].chars().next().expect("valid utf-8");
                    out.push(ch);
                    at_line_start = false;
                    i += ch.len_utf8();
                }
            }
            ScanCtx::CodeSpan { ticks } => {
                let b = bytes[i];
                if b == b'`' {
                    let run = bytes[i..].iter().take_while(|&&c| c == b'`').count();
                    out.push_str(&src[i..i + run]);
                    i += run;
                    if run == ticks {
                        ctx = ScanCtx::Text;
                    }
                    continue;
                }
                if b.is_ascii() {
                    out.push(b as char);
                    at_line_start = b == b'\n';
                    i += 1;
                } else {
                    let ch = src[i..].chars().next().expect("valid utf-8");
                    out.push(ch);
                    at_line_start = false;
                    i += ch.len_utf8();
                }
            }
            ScanCtx::Fence { fence_char } => {
                if at_line_start {
                    let line = &src[i..];
                    let trimmed = line.trim_start_matches(' ');
                    let run_len = trimmed.bytes().take_while(|&b| b == fence_char).count();
                    if run_len >= 3 {
                        let nl = line.find('\n').map_or(line.len(), |n| n + 1);
                        out.push_str(&line[..nl]);
                        i += nl;
                        at_line_start = true;
                        ctx = ScanCtx::Text;
                        continue;
                    }
                }
                let b = bytes[i];
                if b.is_ascii() {
                    out.push(b as char);
                    at_line_start = b == b'\n';
                    i += 1;
                } else {
                    let ch = src[i..].chars().next().expect("valid utf-8");
                    out.push(ch);
                    at_line_start = false;
                    i += ch.len_utf8();
                }
            }
        }
    }

    out
}

/// Convert markdown to HTML, intercepting button syntax in links.
///
/// Strategy: buffer all events between `Start(Link)` and `End(Link)`,
/// then check if the concatenated text matches button syntax.
/// If yes, emit a table-based button. If no, flush all buffered events
/// as a normal link through `push_html`.
pub fn markdown_to_html(markdown: &str, brand_color: Option<&str>) -> String {
    let preprocessed = transform_otp(markdown, otp::render_otp_html);
    let parser = Parser::new_ext(&preprocessed, Options::all());
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
            Event::End(TagEnd::Item) if !text.ends_with('\n') => {
                text.push('\n');
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

    #[test]
    fn html_otp_basic() {
        let html = markdown_to_html("Your code is [otp|123456] — enter it.", None);
        assert!(html.contains("font-family:ui-monospace"));
        assert!(html.contains(">123456<"));
        assert!(html.contains("Your code is"));
        assert!(html.contains("enter it"));
    }

    #[test]
    fn html_otp_alphanumeric_with_hyphen() {
        let html = markdown_to_html("[otp|ABCD-1234]", None);
        assert!(html.contains(">ABCD-1234<"));
    }

    #[test]
    fn html_otp_in_code_span_is_literal() {
        let html = markdown_to_html("Syntax: `[otp|123]`.", None);
        assert!(html.contains("<code>[otp|123]</code>"));
        assert!(!html.contains("font-family:ui-monospace"));
    }

    #[test]
    fn html_otp_in_fenced_block_is_literal() {
        let html = markdown_to_html("```\n[otp|123]\n```", None);
        assert!(html.contains("[otp|123]"));
        assert!(!html.contains("font-family:ui-monospace"));
    }

    #[test]
    fn html_otp_escaped_is_literal() {
        let html = markdown_to_html(r"\[otp|123]", None);
        assert!(html.contains("[otp|123]"));
        assert!(!html.contains("font-family:ui-monospace"));
    }

    #[test]
    fn html_otp_empty_code_is_literal() {
        let html = markdown_to_html("[otp|]", None);
        assert!(html.contains("[otp|]"));
        assert!(!html.contains("font-family:ui-monospace"));
    }

    #[test]
    fn html_otp_with_space_is_literal() {
        let html = markdown_to_html("[otp|12 34]", None);
        assert!(html.contains("[otp|12 34]"));
        assert!(!html.contains("font-family:ui-monospace"));
    }

    #[test]
    fn html_otp_too_long_is_literal() {
        let long = "A".repeat(33);
        let src = format!("[otp|{long}]");
        let html = markdown_to_html(&src, None);
        assert!(!html.contains("font-family:ui-monospace"));
    }

    #[test]
    fn html_otp_multiple_instances() {
        let html = markdown_to_html("[otp|111] and [otp|222]", None);
        assert!(html.contains(">111<"));
        assert!(html.contains(">222<"));
    }
}
