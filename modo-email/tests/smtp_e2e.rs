#![cfg(feature = "smtp")]

use mailin_embedded::{Handler, Server, SslConfig, response};
use modo_email::template::filesystem::FilesystemProvider;
use modo_email::template::layout::LayoutEngine;
use modo_email::transport::smtp::SmtpTransport;
use modo_email::{
    MailMessage, MailTransport, Mailer, SendEmail, SenderProfile, SmtpConfig, SmtpSecurity,
    TemplateProvider,
};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Embedded SMTP server infrastructure
// ---------------------------------------------------------------------------

struct CapturedEmail {
    from: String,
    to: Vec<String>,
    data: String, // raw RFC 822
}

#[derive(Clone)]
struct SmtpCapture {
    emails: Arc<Mutex<Vec<CapturedEmail>>>,
    current_from: Arc<Mutex<String>>,
    current_to: Arc<Mutex<Vec<String>>>,
    current_data: Arc<Mutex<Vec<u8>>>,
}

impl Handler for SmtpCapture {
    fn helo(
        &mut self,
        _ip: std::net::IpAddr,
        _domain: &str,
    ) -> mailin_embedded::response::Response {
        response::OK
    }

    fn mail(
        &mut self,
        _ip: std::net::IpAddr,
        _domain: &str,
        from: &str,
    ) -> mailin_embedded::response::Response {
        *self.current_from.lock().unwrap() = from.to_string();
        response::OK
    }

    fn rcpt(&mut self, to: &str) -> mailin_embedded::response::Response {
        self.current_to.lock().unwrap().push(to.to_string());
        response::OK
    }

    fn data_start(
        &mut self,
        _domain: &str,
        _from: &str,
        _is8bit: bool,
        _to: &[String],
    ) -> mailin_embedded::response::Response {
        self.current_data.lock().unwrap().clear();
        response::START_DATA
    }

    fn data(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.current_data.lock().unwrap().extend_from_slice(buf);
        Ok(())
    }

    fn data_end(&mut self) -> mailin_embedded::response::Response {
        let from = self.current_from.lock().unwrap().clone();
        let to = self.current_to.lock().unwrap().drain(..).collect();
        let data = String::from_utf8_lossy(&self.current_data.lock().unwrap()).to_string();
        self.emails
            .lock()
            .unwrap()
            .push(CapturedEmail { from, to, data });
        response::OK
    }
}

/// Start an embedded SMTP server on an OS-assigned port.
/// Returns the port and a shared handle to captured emails.
fn start_smtp_server() -> (u16, Arc<Mutex<Vec<CapturedEmail>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let emails: Arc<Mutex<Vec<CapturedEmail>>> = Arc::new(Mutex::new(Vec::new()));
    let handler = SmtpCapture {
        emails: emails.clone(),
        current_from: Arc::new(Mutex::new(String::new())),
        current_to: Arc::new(Mutex::new(Vec::new())),
        current_data: Arc::new(Mutex::new(Vec::new())),
    };

    std::thread::spawn(move || {
        let mut server = Server::new(handler);
        server
            .with_name("localhost")
            .with_ssl(SslConfig::None)
            .unwrap()
            .with_tcp_listener(listener);
        // serve() blocks; the thread exits when the test process ends.
        server.serve().ok();
    });

    (port, emails)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smtp_end_to_end() {
    let (port, emails) = start_smtp_server();

    let transport = SmtpTransport::new(&SmtpConfig {
        host: "localhost".to_string(),
        port,
        username: String::new(),
        password: String::new(),
        security: SmtpSecurity::None,
    })
    .unwrap();

    let message = MailMessage {
        from: "Sender <sender@test.com>".to_string(),
        reply_to: Some("reply@test.com".to_string()),
        to: vec!["recipient@test.com".to_string()],
        subject: "Test Subject".to_string(),
        html: "<h1>Hello</h1>".to_string(),
        text: "Hello".to_string(),
    };

    transport.send(&message).await.unwrap();

    let captured = emails.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert!(
        captured[0].from.contains("sender@test.com"),
        "envelope from should contain sender address"
    );
    assert!(
        captured[0]
            .to
            .iter()
            .any(|t| t.contains("recipient@test.com")),
        "envelope to should contain recipient address"
    );

    let raw = &captured[0].data;
    assert!(
        raw.contains("Subject: Test Subject"),
        "raw data should contain Subject header"
    );
    assert!(
        raw.contains("text/plain"),
        "raw data should contain text/plain part"
    );
    assert!(
        raw.contains("text/html"),
        "raw data should contain text/html part"
    );
    assert!(
        raw.contains("Reply-To:"),
        "raw data should contain Reply-To header"
    );
}

#[tokio::test]
async fn smtp_full_pipeline_end_to_end() {
    let (port, emails) = start_smtp_server();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    std::fs::write(
        path.join("welcome.md"),
        "---\nsubject: \"Welcome {{name}}!\"\n---\nHi **{{name}}**, glad to have you.",
    )
    .unwrap();

    let transport = Arc::new(
        SmtpTransport::new(&SmtpConfig {
            host: "localhost".to_string(),
            port,
            username: String::new(),
            password: String::new(),
            security: SmtpSecurity::None,
        })
        .unwrap(),
    );

    let provider: Arc<dyn TemplateProvider> =
        Arc::new(FilesystemProvider::new(path.to_str().unwrap()));
    let layout = Arc::new(LayoutEngine::new(path.to_str().unwrap()));

    let mailer = Mailer::new(
        transport,
        provider,
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        layout,
    );

    mailer
        .send(&SendEmail::new("welcome", "user@test.com").var("name", "Alice"))
        .await
        .unwrap();

    let captured = emails.lock().unwrap();
    assert_eq!(captured.len(), 1);

    let raw = &captured[0].data;
    assert!(
        raw.contains("Welcome Alice!"),
        "raw data should contain substituted subject"
    );
    assert!(
        raw.contains("text/plain"),
        "raw data should contain text/plain part"
    );
    assert!(
        raw.contains("text/html"),
        "raw data should contain text/html part"
    );
}
