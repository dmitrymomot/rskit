#[test]
fn test_trim() {
    let mut s = "  hello world  ".to_string();
    modo::sanitize::trim(&mut s);
    assert_eq!(s, "hello world");
}

#[test]
fn test_trim_lowercase() {
    let mut s = "  Hello WORLD  ".to_string();
    modo::sanitize::trim_lowercase(&mut s);
    assert_eq!(s, "hello world");
}

#[test]
fn test_collapse_whitespace() {
    let mut s = "hello   world\n\tfoo".to_string();
    modo::sanitize::collapse_whitespace(&mut s);
    assert_eq!(s, "hello world foo");
}

#[test]
fn test_strip_html() {
    let mut s = "<p>Hello <b>world</b></p>".to_string();
    modo::sanitize::strip_html(&mut s);
    assert_eq!(s.trim(), "Hello world");
}

#[test]
fn test_strip_html_entities() {
    let mut s = "&amp; &lt;b&gt;bold&lt;/b&gt;".to_string();
    modo::sanitize::strip_html(&mut s);
    assert!(s.contains("&"));
    assert!(!s.contains("&amp;"));
}

#[test]
fn test_truncate() {
    let mut s = "hello world".to_string();
    modo::sanitize::truncate(&mut s, 5);
    assert_eq!(s, "hello");
}

#[test]
fn test_truncate_no_op_if_shorter() {
    let mut s = "hi".to_string();
    modo::sanitize::truncate(&mut s, 10);
    assert_eq!(s, "hi");
}

#[test]
fn test_truncate_respects_char_boundaries() {
    let mut s = "héllo".to_string();
    modo::sanitize::truncate(&mut s, 2);
    assert_eq!(s, "hé");
}

#[test]
fn test_normalize_email() {
    let mut s = "  User+Tag@Example.COM  ".to_string();
    modo::sanitize::normalize_email(&mut s);
    assert_eq!(s, "user@example.com");
}

#[test]
fn test_normalize_email_no_plus() {
    let mut s = "USER@EXAMPLE.COM".to_string();
    modo::sanitize::normalize_email(&mut s);
    assert_eq!(s, "user@example.com");
}

#[test]
fn test_strip_html_removes_script_tags() {
    // Script content (including the tag itself) must be completely discarded.
    let mut s = "<p>Hello</p><script>alert('xss')</script><p>World</p>".to_string();
    modo::sanitize::strip_html(&mut s);
    // The <p> tags produce a separating space; script content is gone entirely.
    assert!(!s.contains("script"), "script tag must be removed");
    assert!(!s.contains("alert"), "script content must be removed");
    assert!(s.contains("Hello"), "Hello must be preserved");
    assert!(s.contains("World"), "World must be preserved");
    // Collapsed output should be "Hello World"
    assert_eq!(s.trim(), "Hello World");
}

#[test]
fn test_sanitize_trait() {
    use modo::sanitize::Sanitize;

    struct Input {
        name: String,
        email: String,
    }
    impl Sanitize for Input {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.name);
            modo::sanitize::normalize_email(&mut self.email);
        }
    }

    let mut input = Input {
        name: "  Alice  ".to_string(),
        email: "Alice+work@Gmail.COM".to_string(),
    };
    input.sanitize();
    assert_eq!(input.name, "Alice");
    assert_eq!(input.email, "alice@gmail.com");
}
