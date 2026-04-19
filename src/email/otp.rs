use crate::email::render;

/// Character class accepted as OTP code body: ASCII letters, digits, hyphen.
/// Length 1..=32.
pub(crate) fn is_valid_code(s: &str) -> bool {
    let len = s.len();
    if !(1..=32).contains(&len) {
        return false;
    }
    s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Render a styled HTML OTP pill (table-based, fully inline styles).
///
/// The code is HTML-escaped before interpolation.
pub fn render_otp_html(code: &str) -> String {
    let escaped = render::escape_html(code);
    format!(
        r#"<table role="presentation" border="0" cellpadding="0" cellspacing="0" style="margin:8px 0 24px 0;"><tr><td style="font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;font-size:28px;font-weight:700;letter-spacing:6px;color:#18181b;background-color:#f4f4f5;padding:14px 20px;border-radius:8px;">{escaped}</td></tr></table>"#
    )
}

/// Plain-text OTP rendering: blank line, code, blank line.
///
/// Returns a string with leading and trailing `\n\n` so it sits as its own
/// block in surrounding paragraph flow.
#[allow(dead_code)] // wired up in markdown_to_text (Task 5)
pub fn render_otp_text(code: &str) -> String {
    format!("\n\n{code}\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_code_accepts_digits() {
        assert!(is_valid_code("123456"));
    }

    #[test]
    fn is_valid_code_accepts_alphanumeric_with_hyphen() {
        assert!(is_valid_code("ABCD-1234"));
    }

    #[test]
    fn is_valid_code_rejects_empty() {
        assert!(!is_valid_code(""));
    }

    #[test]
    fn is_valid_code_rejects_too_long() {
        assert!(!is_valid_code(&"A".repeat(33)));
    }

    #[test]
    fn is_valid_code_accepts_max_length() {
        assert!(is_valid_code(&"A".repeat(32)));
    }

    #[test]
    fn is_valid_code_rejects_space() {
        assert!(!is_valid_code("123 456"));
    }

    #[test]
    fn is_valid_code_rejects_punctuation() {
        assert!(!is_valid_code("abc.def"));
        assert!(!is_valid_code("abc]def"));
    }

    #[test]
    fn render_html_basic() {
        let html = render_otp_html("123456");
        assert!(html.contains(">123456<"));
        assert!(html.contains("role=\"presentation\""));
        assert!(html.contains("font-family:ui-monospace"));
        assert!(html.contains("letter-spacing:6px"));
    }

    #[test]
    fn render_html_escapes_code() {
        let html = render_otp_html("<b>&");
        assert!(html.contains("&lt;b&gt;&amp;"));
        assert!(!html.contains("<b>"));
    }

    #[test]
    fn render_text_format() {
        assert_eq!(render_otp_text("123456"), "\n\n123456\n\n");
    }
}
