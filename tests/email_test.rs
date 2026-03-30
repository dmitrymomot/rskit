#![cfg(all(feature = "email", feature = "test-helpers"))]

use modo::email::{EmailConfig, Mailer, SendEmail};
use std::collections::HashMap;

fn test_config(dir: &std::path::Path) -> EmailConfig {
    let mut c = EmailConfig::default();
    c.templates_path = dir.to_str().unwrap().into();
    c.layouts_path = dir.join("layouts").to_str().unwrap().into();
    c.default_from_name = "TestApp".into();
    c.default_from_email = "noreply@test.com".into();
    c.default_reply_to = Some("support@test.com".into());
    c.default_locale = "en".into();
    c.cache_templates = false;
    c
}

fn write_template(dir: &std::path::Path, locale: &str, name: &str, content: &str) {
    let locale_dir = dir.join(locale);
    std::fs::create_dir_all(&locale_dir).unwrap();
    std::fs::write(locale_dir.join(format!("{name}.md")), content).unwrap();
}

#[test]
fn render_basic_template() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "welcome",
        "---\nsubject: \"Welcome {{name}}!\"\n---\nHi **{{name}}**, welcome!",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("welcome", "user@example.com").var("name", "Dmytro");

    let rendered = mailer.render(&email).unwrap();
    assert_eq!(rendered.subject, "Welcome Dmytro!");
    assert!(rendered.html.contains("<strong>Dmytro</strong>"));
    assert!(rendered.html.contains("max-width: 600px")); // base layout applied
    assert!(rendered.text.contains("Hi Dmytro, welcome!"));
}

#[test]
fn render_with_button() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "action",
        "---\nsubject: Action needed\n---\n[button:danger|Delete](https://example.com/del)",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("action", "user@example.com");
    let rendered = mailer.render(&email).unwrap();
    assert!(rendered.html.contains("background-color: #dc2626")); // danger color
    assert!(rendered.text.contains("Delete: https://example.com/del"));
}

#[test]
fn render_with_custom_layout() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "custom",
        "---\nsubject: Custom\nlayout: simple\n---\nBody here",
    );
    std::fs::create_dir_all(dir.path().join("layouts")).unwrap();
    std::fs::write(
        dir.path().join("layouts/simple.html"),
        "<html><body>{{content}}</body></html>",
    )
    .unwrap();

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("custom", "user@example.com");
    let rendered = mailer.render(&email).unwrap();
    assert!(rendered.html.starts_with("<html>"));
    assert!(rendered.html.contains("Body here"));
    assert!(!rendered.html.contains("max-width: 600px")); // not base layout
}

#[test]
fn render_locale_fallback() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "greeting",
        "---\nsubject: English Greeting\n---\nHello!",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    // Request French, falls back to English
    let email = SendEmail::new("greeting", "user@example.com").locale("fr");
    let rendered = mailer.render(&email).unwrap();
    assert_eq!(rendered.subject, "English Greeting");
}

#[tokio::test]
async fn send_with_stub_transport() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "welcome",
        "---\nsubject: Welcome!\n---\nHello!",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub.clone()).unwrap();

    mailer
        .send(
            SendEmail::new("welcome", "user@example.com")
                .cc("cc@example.com")
                .bcc("bcc@example.com"),
        )
        .await
        .unwrap();

    let msgs = stub.messages().await;
    assert_eq!(msgs.len(), 1);
    let (envelope, raw) = &msgs[0];
    assert!(
        envelope
            .to()
            .iter()
            .any(|a| AsRef::<str>::as_ref(a) == "user@example.com")
    );
    assert!(raw.contains("Subject: Welcome!"));
}

#[tokio::test]
async fn send_empty_to_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    write_template(dir.path(), "en", "test", "---\nsubject: Test\n---\nBody");

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail {
        template: "test".into(),
        to: vec![],
        cc: vec![],
        bcc: vec![],
        locale: None,
        vars: HashMap::new(),
        sender: None,
    };

    let result = mailer.send(email).await;
    assert!(result.is_err());
}

#[test]
fn render_template_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("nonexistent", "user@example.com");
    let result = mailer.render(&email);
    assert!(result.is_err());
}

#[test]
fn render_with_cache_enabled() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "cached",
        "---\nsubject: Cached!\n---\nBody",
    );

    let config = {
        let mut c = EmailConfig::default();
        c.templates_path = dir.path().to_str().unwrap().into();
        c.layouts_path = dir.path().join("layouts").to_str().unwrap().into();
        c.default_from_name = "TestApp".into();
        c.default_from_email = "noreply@test.com".into();
        c.default_reply_to = None;
        c.default_locale = "en".into();
        c.cache_templates = true;
        c.template_cache_size = 10;
        c
    };
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("cached", "user@example.com");

    // First render — cache miss
    let r1 = mailer.render(&email).unwrap();
    assert_eq!(r1.subject, "Cached!");

    // Second render — cache hit, same result
    let r2 = mailer.render(&email).unwrap();
    assert_eq!(r2.subject, "Cached!");
    assert_eq!(r1.html, r2.html);
}

#[tokio::test]
async fn send_with_custom_sender_profile() {
    use modo::email::SenderProfile;

    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "notify",
        "---\nsubject: Notice\n---\nHello",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub.clone()).unwrap();

    mailer
        .send(
            SendEmail::new("notify", "user@example.com").sender(SenderProfile {
                from_name: "Custom".into(),
                from_email: "custom@example.com".into(),
                reply_to: Some("reply@example.com".into()),
            }),
        )
        .await
        .unwrap();

    let msgs = stub.messages().await;
    assert_eq!(msgs.len(), 1);
    let (_envelope, raw) = &msgs[0];
    assert!(raw.contains("custom@example.com"));
}
