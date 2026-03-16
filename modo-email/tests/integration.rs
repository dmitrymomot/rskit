use modo_email::template::filesystem::FilesystemProvider;
use modo_email::template::layout::LayoutEngine;
use modo_email::{
    MailMessage, MailTransportDyn, MailTransportSend, Mailer, SendEmail, SenderProfile,
};
use std::sync::{Arc, Mutex};

/// A transport that captures sent messages for assertions.
struct CapturingTransport {
    messages: Mutex<Vec<MailMessage>>,
}

/// A transport that always returns an error.
struct FailingTransport;

impl MailTransportSend for CapturingTransport {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
        self.messages.lock().unwrap().push(message.clone());
        Ok(())
    }
}

impl MailTransportSend for FailingTransport {
    async fn send(&self, _message: &MailMessage) -> Result<(), modo::Error> {
        Err(modo::Error::internal("connection refused"))
    }
}

#[tokio::test]
async fn end_to_end_filesystem_template() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    // Create a template with frontmatter, variable placeholders, and a button link.
    std::fs::write(
        path.join("welcome.md"),
        "---\nsubject: \"Welcome {{name}}!\"\n---\n\nHi **{{name}}**,\n\nGet started:\n\n[button|Launch Dashboard]({{url}})\n",
    )
    .unwrap();

    let transport = Arc::new(CapturingTransport {
        messages: Mutex::new(Vec::new()),
    });
    let provider: Arc<dyn modo_email::TemplateProvider> =
        Arc::new(FilesystemProvider::new(path.to_str().unwrap()));
    let layout = Arc::new(LayoutEngine::new(path.to_str().unwrap()));

    let mailer = Mailer::new(
        transport.clone(),
        provider,
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        layout,
    );

    mailer
        .send(
            &SendEmail::new("welcome", "user@example.com")
                .var("name", "Alice")
                .var("url", "https://app.com/dashboard"),
        )
        .await
        .unwrap();

    let msgs = transport.messages.lock().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].subject, "Welcome Alice!");
    assert!(msgs[0].html.contains("Alice"));
    assert!(msgs[0].html.contains("Launch Dashboard"));
    assert!(msgs[0].html.contains("https://app.com/dashboard"));
    assert!(msgs[0].html.contains("role=\"presentation\"")); // button rendered
    assert!(
        msgs[0]
            .text
            .contains("Launch Dashboard (https://app.com/dashboard)")
    );
}

#[tokio::test]
async fn end_to_end_locale_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    // Root template only — no locale-specific variant.
    std::fs::write(
        path.join("reset.md"),
        "---\nsubject: \"Reset password\"\n---\n\nClick below to reset.",
    )
    .unwrap();

    let transport = Arc::new(CapturingTransport {
        messages: Mutex::new(Vec::new()),
    });
    let mailer = Mailer::new(
        transport.clone(),
        Arc::new(FilesystemProvider::new(path.to_str().unwrap())),
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        Arc::new(LayoutEngine::new(path.to_str().unwrap())),
    );

    // Request "fr" locale — should fall back to root template.
    mailer
        .send(&SendEmail::new("reset", "user@example.com").locale("fr"))
        .await
        .unwrap();

    let msgs = transport.messages.lock().unwrap();
    assert_eq!(msgs[0].subject, "Reset password");
}

#[tokio::test]
async fn subject_appears_in_layout_title() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    std::fs::write(
        path.join("greeting.md"),
        "---\nsubject: \"Hello {{name}}!\"\n---\nBody.",
    )
    .unwrap();

    let transport = Arc::new(CapturingTransport {
        messages: Mutex::new(Vec::new()),
    });
    let mailer = Mailer::new(
        transport.clone(),
        Arc::new(FilesystemProvider::new(path.to_str().unwrap())),
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        Arc::new(LayoutEngine::new(path.to_str().unwrap())),
    );

    mailer
        .send(&SendEmail::new("greeting", "user@example.com").var("name", "Alice"))
        .await
        .unwrap();

    let msgs = transport.messages.lock().unwrap();
    assert!(
        msgs[0].html.contains("<title>Hello Alice!</title>"),
        "substituted subject should appear in layout <title>"
    );
}

#[tokio::test]
async fn context_vars_flow_to_layout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    std::fs::write(
        path.join("welcome2.md"),
        "---\nsubject: \"Welcome\"\n---\nHello!",
    )
    .unwrap();

    let transport = Arc::new(CapturingTransport {
        messages: Mutex::new(Vec::new()),
    });
    let mailer = Mailer::new(
        transport.clone(),
        Arc::new(FilesystemProvider::new(path.to_str().unwrap())),
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        Arc::new(LayoutEngine::new(path.to_str().unwrap())),
    );

    mailer
        .send(
            &SendEmail::new("welcome2", "user@example.com")
                .var("logo_url", "https://cdn.example.com/logo.png")
                .var("product_name", "Acme")
                .var("footer_text", "Copyright 2026"),
        )
        .await
        .unwrap();

    let msgs = transport.messages.lock().unwrap();
    assert!(
        msgs[0].html.contains("<img"),
        "logo_url should produce <img> tag"
    );
    assert!(
        msgs[0].html.contains("cdn.example.com/logo.png"),
        "logo_url value should appear in img src"
    );
    assert!(
        msgs[0].html.contains("Copyright 2026"),
        "footer_text should flow to layout footer"
    );
}

#[tokio::test]
async fn custom_layout_from_filesystem() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    // Create a custom layout file.
    std::fs::create_dir_all(path.join("layouts")).unwrap();
    std::fs::write(
        path.join("layouts/branded.html"),
        "<html><title>{{subject}}</title><body><main>{{content}}</main>\
         <footer>{{footer_text | default(value=\"\")}}</footer></body></html>",
    )
    .unwrap();

    std::fs::write(
        path.join("hi.md"),
        "---\nsubject: \"Hi\"\nlayout: branded\n---\nBody {{name}}",
    )
    .unwrap();

    let transport = Arc::new(CapturingTransport {
        messages: Mutex::new(Vec::new()),
    });
    let mailer = Mailer::new(
        transport.clone(),
        Arc::new(FilesystemProvider::new(path.to_str().unwrap())),
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        Arc::new(LayoutEngine::new(path.to_str().unwrap())),
    );

    mailer
        .send(&SendEmail::new("hi", "user@example.com").var("name", "Bob"))
        .await
        .unwrap();

    let msgs = transport.messages.lock().unwrap();
    assert!(
        msgs[0].html.contains("<main>"),
        "custom layout should be used"
    );
    assert!(
        !msgs[0].html.contains("max-width"),
        "default layout should NOT be used"
    );
}

#[tokio::test]
async fn transport_error_propagates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    std::fs::write(path.join("err.md"), "---\nsubject: \"Test\"\n---\nBody.").unwrap();

    let transport: Arc<dyn MailTransportDyn> = Arc::new(FailingTransport);
    let mailer = Mailer::new(
        transport,
        Arc::new(FilesystemProvider::new(path.to_str().unwrap())),
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        Arc::new(LayoutEngine::new(path.to_str().unwrap())),
    );

    let result = mailer
        .send(&SendEmail::new("err", "user@example.com"))
        .await;
    assert!(result.is_err(), "transport error should propagate");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("connection refused"),
        "error should contain transport message, got: {err_msg}"
    );
}

#[tokio::test]
async fn multiple_recipients_in_rendered_message() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    std::fs::write(
        path.join("notify.md"),
        "---\nsubject: \"Notify\"\n---\nHello team.",
    )
    .unwrap();

    let transport = Arc::new(CapturingTransport {
        messages: Mutex::new(Vec::new()),
    });
    let mailer = Mailer::new(
        transport.clone(),
        Arc::new(FilesystemProvider::new(path.to_str().unwrap())),
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        Arc::new(LayoutEngine::new(path.to_str().unwrap())),
    );

    mailer
        .send(
            &SendEmail::new("notify", "a@t.com")
                .to("b@t.com")
                .to("c@t.com"),
        )
        .await
        .unwrap();

    let msgs = transport.messages.lock().unwrap();
    assert_eq!(msgs[0].to.len(), 3);
    assert!(msgs[0].to.contains(&"a@t.com".to_string()));
    assert!(msgs[0].to.contains(&"b@t.com".to_string()));
    assert!(msgs[0].to.contains(&"c@t.com".to_string()));
}
