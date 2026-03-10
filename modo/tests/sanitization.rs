use modo::sanitize::Sanitize;
use modo::validate::Validate;

// =============================================================================
// Built-in rule tests
// =============================================================================

#[derive(serde::Deserialize, modo::Sanitize)]
struct TrimField {
    #[clean(trim)]
    name: String,
}

#[test]
fn trim_strips_whitespace() {
    let mut v = TrimField {
        name: "  hello  ".into(),
    };
    v.sanitize();
    assert_eq!(v.name, "hello");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct LowercaseField {
    #[clean(lowercase)]
    name: String,
}

#[test]
fn lowercase_converts() {
    let mut v = LowercaseField {
        name: "HELLO World".into(),
    };
    v.sanitize();
    assert_eq!(v.name, "hello world");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct UppercaseField {
    #[clean(uppercase)]
    name: String,
}

#[test]
fn uppercase_converts() {
    let mut v = UppercaseField {
        name: "hello World".into(),
    };
    v.sanitize();
    assert_eq!(v.name, "HELLO WORLD");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct StripHtmlField {
    #[clean(strip_html)]
    content: String,
}

#[test]
fn strip_html_removes_tags() {
    let mut v = StripHtmlField {
        content: "<p>Hello <b>world</b></p>".into(),
    };
    v.sanitize();
    assert_eq!(v.content, "Hello world");
}

#[test]
fn strip_html_nested_tags() {
    let mut v = StripHtmlField {
        content: "<div><span>nested</span></div>".into(),
    };
    v.sanitize();
    assert_eq!(v.content, "nested");
}

#[test]
fn strip_html_no_tags() {
    let mut v = StripHtmlField {
        content: "plain text".into(),
    };
    v.sanitize();
    assert_eq!(v.content, "plain text");
}

#[test]
fn strip_html_self_closing() {
    let mut v = StripHtmlField {
        content: "line<br/>break".into(),
    };
    v.sanitize();
    assert_eq!(v.content, "linebreak");
}

#[test]
fn strip_html_script_content() {
    let mut v = StripHtmlField {
        content: "<script>alert('xss')</script>safe".into(),
    };
    v.sanitize();
    assert_eq!(v.content, "alert('xss')safe");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct CollapseWsField {
    #[clean(collapse_whitespace)]
    text: String,
}

#[test]
fn collapse_whitespace_works() {
    let mut v = CollapseWsField {
        text: "hello   world\t\nfoo".into(),
    };
    v.sanitize();
    assert_eq!(v.text, "hello world foo");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct TruncateField {
    #[clean(truncate = 5)]
    text: String,
}

#[test]
fn truncate_ascii() {
    let mut v = TruncateField {
        text: "abcdefgh".into(),
    };
    v.sanitize();
    assert_eq!(v.text, "abcde");
}

#[test]
fn truncate_shorter_than_limit() {
    let mut v = TruncateField { text: "abc".into() };
    v.sanitize();
    assert_eq!(v.text, "abc");
}

#[test]
fn truncate_multibyte_utf8() {
    // Each emoji is one char but multiple bytes
    let mut v = TruncateField {
        text: "\u{1F600}\u{1F601}\u{1F602}\u{1F603}\u{1F604}\u{1F605}\u{1F606}".into(),
    };
    v.sanitize();
    assert_eq!(v.text.chars().count(), 5);
    assert_eq!(v.text, "\u{1F600}\u{1F601}\u{1F602}\u{1F603}\u{1F604}");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct NormalizeEmailField {
    #[clean(normalize_email)]
    email: String,
}

#[test]
fn normalize_email_strips_plus_tag() {
    let mut v = NormalizeEmailField {
        email: "user+tag@example.com".into(),
    };
    v.sanitize();
    assert_eq!(v.email, "user@example.com");
}

#[test]
fn normalize_email_no_plus() {
    let mut v = NormalizeEmailField {
        email: "user@example.com".into(),
    };
    v.sanitize();
    assert_eq!(v.email, "user@example.com");
}

#[test]
fn normalize_email_no_at() {
    let mut v = NormalizeEmailField {
        email: "not-an-email".into(),
    };
    v.sanitize();
    assert_eq!(v.email, "not-an-email");
}

#[test]
fn normalize_email_plus_in_domain_only() {
    let mut v = NormalizeEmailField {
        email: "user@exam+ple.com".into(),
    };
    v.sanitize();
    // No `+` in local part, so only lowercased
    assert_eq!(v.email, "user@exam+ple.com");
}

// =============================================================================
// Edge-case tests
// =============================================================================

#[test]
fn empty_string_inputs() {
    use modo::sanitize;

    assert_eq!(sanitize::trim(String::new()), "");
    assert_eq!(sanitize::strip_html_tags(String::new()), "");
    assert_eq!(sanitize::truncate(String::new(), 5), "");
    assert_eq!(sanitize::lowercase(String::new()), "");
    assert_eq!(sanitize::uppercase(String::new()), "");
    assert_eq!(sanitize::collapse_whitespace(String::new()), "");
    assert_eq!(sanitize::normalize_email(String::new()), "");
}

#[test]
fn strip_html_unclosed_tag() {
    let mut v = StripHtmlField {
        content: "text<b".into(),
    };
    v.sanitize();
    // Unclosed tag at end swallows content after `<`
    assert_eq!(v.content, "text");
}

#[test]
fn strip_html_angle_brackets_in_text() {
    let mut v = StripHtmlField {
        content: "3 > 2".into(),
    };
    v.sanitize();
    // `>` ends the "tag" opened by nothing — false-positive strips " 2"
    assert_eq!(v.content, "3  2");
}

#[test]
fn collapse_whitespace_already_clean() {
    let mut v = CollapseWsField {
        text: "hello world".into(),
    };
    v.sanitize();
    assert_eq!(v.text, "hello world");
}

#[test]
fn collapse_whitespace_leading_trailing() {
    let mut v = CollapseWsField {
        text: "  hello  ".into(),
    };
    v.sanitize();
    // Leading/trailing whitespace collapsed to single spaces
    assert_eq!(v.text, " hello ");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct TruncateZeroField {
    #[clean(truncate = 0)]
    text: String,
}

#[test]
fn truncate_zero() {
    let mut v = TruncateZeroField {
        text: "hello".into(),
    };
    v.sanitize();
    assert_eq!(v.text, "");
}

#[test]
fn normalize_email_multiple_plus() {
    let mut v = NormalizeEmailField {
        email: "u+a+b@x.com".into(),
    };
    v.sanitize();
    assert_eq!(v.email, "u@x.com");
}

#[test]
fn normalize_email_full_lowercasing() {
    let mut v = NormalizeEmailField {
        email: "User@Example.COM".into(),
    };
    v.sanitize();
    assert_eq!(v.email, "user@example.com");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct CustomOptionField {
    #[clean(custom = "reverse_string")]
    text: Option<String>,
}

#[test]
fn custom_function_on_option() {
    let mut v = CustomOptionField {
        text: Some("hello".into()),
    };
    v.sanitize();
    assert_eq!(v.text, Some("olleh".into()));

    let mut v = CustomOptionField { text: None };
    v.sanitize();
    assert!(v.text.is_none());
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct MultiAttrField {
    #[clean(trim)]
    #[clean(lowercase)]
    name: String,
}

#[test]
fn multiple_clean_attributes_on_one_field() {
    let mut v = MultiAttrField {
        name: "  HELLO  ".into(),
    };
    v.sanitize();
    assert_eq!(v.name, "hello");
}

// =============================================================================
// Option<String> support
// =============================================================================

#[derive(serde::Deserialize, modo::Sanitize)]
struct OptionField {
    #[clean(trim, lowercase)]
    name: Option<String>,
}

#[test]
fn option_none_stays_none() {
    let mut v = OptionField { name: None };
    v.sanitize();
    assert!(v.name.is_none());
}

#[test]
fn option_some_gets_sanitized() {
    let mut v = OptionField {
        name: Some("  HELLO  ".into()),
    };
    v.sanitize();
    assert_eq!(v.name, Some("hello".into()));
}

// =============================================================================
// Multiple rules — left-to-right order
// =============================================================================

#[derive(serde::Deserialize, modo::Sanitize)]
struct MultiRule {
    #[clean(trim, lowercase)]
    username: String,
}

#[test]
fn multiple_rules_left_to_right() {
    let mut v = MultiRule {
        username: "  ALICE  ".into(),
    };
    v.sanitize();
    assert_eq!(v.username, "alice");
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct TrimThenTruncate {
    #[clean(trim, truncate = 3)]
    text: String,
}

#[test]
fn trim_then_truncate_order_matters() {
    let mut v = TrimThenTruncate {
        text: "  abcdef  ".into(),
    };
    v.sanitize();
    // trim first -> "abcdef", then truncate to 3 -> "abc"
    assert_eq!(v.text, "abc");
}

// =============================================================================
// Custom function
// =============================================================================

fn reverse_string(s: String) -> String {
    s.chars().rev().collect()
}

#[derive(serde::Deserialize, modo::Sanitize)]
struct CustomField {
    #[clean(custom = "reverse_string")]
    text: String,
}

#[test]
fn custom_function_applied() {
    let mut v = CustomField {
        text: "hello".into(),
    };
    v.sanitize();
    assert_eq!(v.text, "olleh");
}

// =============================================================================
// No-attrs struct — no-op
// =============================================================================

#[allow(dead_code)]
#[derive(serde::Deserialize, modo::Sanitize)]
struct NoSanitizeAttrs {
    name: String,
    age: i32,
}

#[test]
fn no_sanitize_attrs_is_noop() {
    let mut v = NoSanitizeAttrs {
        name: "  Hello  ".into(),
        age: 25,
    };
    v.sanitize();
    assert_eq!(v.name, "  Hello  ");
    assert_eq!(v.age, 25);
}

// =============================================================================
// Combined Sanitize + Validate
// =============================================================================

#[derive(serde::Deserialize, modo::Sanitize, modo::Validate)]
struct CombinedForm {
    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    email: String,

    #[clean(trim, strip_html)]
    #[validate(required, min_length = 5)]
    message: String,
}

#[test]
fn sanitize_then_validate_passes() {
    let mut v = CombinedForm {
        email: "  user+tag@example.com  ".into(),
        message: "  <b>Hello</b> world  ".into(),
    };
    v.sanitize();
    assert_eq!(v.email, "user@example.com");
    assert_eq!(v.message, "Hello world");
    assert!(v.validate().is_ok());
}

#[test]
fn sanitize_then_validate_fails() {
    let mut v = CombinedForm {
        email: "  not-an-email  ".into(),
        message: "  <b>Hi</b>  ".into(),
    };
    v.sanitize();
    assert_eq!(v.email, "not-an-email");
    assert_eq!(v.message, "Hi");
    let err = v.validate().unwrap_err();
    assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    // email fails email validation
    assert!(err.details().contains_key("email"));
    // message fails min_length = 5 ("Hi" is 2 chars)
    assert!(err.details().contains_key("message"));
}
