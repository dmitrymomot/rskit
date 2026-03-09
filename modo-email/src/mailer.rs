use crate::message::{MailMessage, SendEmail, SenderProfile};
use crate::template::layout::LayoutEngine;
use crate::template::{TemplateProvider, markdown, vars};
use crate::transport::MailTransport;
use std::collections::HashMap;
use std::sync::Arc;

/// High-level email service that ties together template loading, variable
/// substitution, Markdown rendering, layout wrapping, and transport delivery.
///
/// `Mailer` is cheaply cloneable via internal `Arc`s, making it safe to share
/// across async tasks and register as a modo service.
#[derive(Clone)]
pub struct Mailer {
    transport: Arc<dyn MailTransport>,
    templates: Arc<dyn TemplateProvider>,
    default_sender: SenderProfile,
    layout_engine: Arc<LayoutEngine>,
}

impl Mailer {
    pub fn new(
        transport: Arc<dyn MailTransport>,
        templates: Arc<dyn TemplateProvider>,
        default_sender: SenderProfile,
        layout_engine: Arc<LayoutEngine>,
    ) -> Self {
        Self {
            transport,
            templates,
            default_sender,
            layout_engine,
        }
    }

    /// Render a `SendEmail` into a fully-formed `MailMessage` without sending.
    pub fn render(&self, email: &SendEmail) -> Result<MailMessage, modo::Error> {
        let locale = email.locale.as_deref().unwrap_or("");
        let template = self.templates.get(&email.template, locale)?;

        // Substitute variables in subject and body.
        let subject = vars::substitute(&template.subject, &email.context);
        let body = vars::substitute(&template.body, &email.context);

        // Validate brand_color as a CSS hex color; fall back to default if invalid.
        let button_color = email
            .context
            .get("brand_color")
            .and_then(|v| v.as_str())
            .filter(|s| is_valid_hex_color(s))
            .unwrap_or("#4F46E5");

        // Render Markdown body to HTML (with optional custom button color).
        let html_body = markdown::render_markdown_with_color(&body, button_color);
        let text = markdown::render_plain_text(&body);

        // Wrap HTML body in a layout.
        let layout_name = template.layout.as_deref().unwrap_or("default");
        let mut layout_ctx: HashMap<String, serde_json::Value> = email.context.clone();
        layout_ctx.insert("content".to_string(), serde_json::Value::String(html_body));
        layout_ctx.insert(
            "subject".to_string(),
            serde_json::Value::String(subject.clone()),
        );
        let html = self.layout_engine.render(layout_name, &layout_ctx)?;

        // Resolve sender (per-email override or default).
        let sender = email.sender.as_ref().unwrap_or(&self.default_sender);

        Ok(MailMessage {
            from: sender.format_address(),
            reply_to: sender.reply_to.clone(),
            to: email.to.clone(),
            subject,
            html,
            text,
        })
    }

    /// Render and deliver an email via the configured transport.
    pub async fn send(&self, email: &SendEmail) -> Result<(), modo::Error> {
        let message = self.render(email)?;
        self.transport.send(&message).await
    }
}

/// Validate that a string is a valid CSS hex color (#RGB or #RRGGBB).
fn is_valid_hex_color(s: &str) -> bool {
    let s = s.as_bytes();
    matches!(s.len(), 4 | 7) && s[0] == b'#' && s[1..].iter().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::EmailTemplate;

    struct MockTransport {
        sent: std::sync::Mutex<Vec<MailMessage>>,
    }

    #[async_trait::async_trait]
    impl MailTransport for MockTransport {
        async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
            self.sent.lock().unwrap().push(message.clone());
            Ok(())
        }
    }

    struct MockTemplateProvider;

    impl TemplateProvider for MockTemplateProvider {
        fn get(&self, _name: &str, _locale: &str) -> Result<EmailTemplate, modo::Error> {
            Ok(EmailTemplate {
                subject: "Hello {{name}}".to_string(),
                body: "Hi **{{name}}**!\n\n[button|Click](https://example.com)".to_string(),
                layout: None,
            })
        }
    }

    fn test_mailer(transport: Arc<dyn MailTransport>) -> Mailer {
        Mailer::new(
            transport,
            Arc::new(MockTemplateProvider),
            SenderProfile {
                from_name: "Test".to_string(),
                from_email: "test@test.com".to_string(),
                reply_to: None,
            },
            Arc::new(LayoutEngine::builtin_only()),
        )
    }

    #[tokio::test]
    async fn send_renders_and_delivers() {
        let transport = Arc::new(MockTransport {
            sent: std::sync::Mutex::new(Vec::new()),
        });
        let mailer = test_mailer(transport.clone());

        mailer
            .send(&SendEmail::new("welcome", "user@test.com").var("name", "Alice"))
            .await
            .unwrap();

        let messages = transport.sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].to, vec!["user@test.com"]);
        assert_eq!(messages[0].subject, "Hello Alice");
        assert!(messages[0].html.contains("Alice"));
        assert!(messages[0].html.contains("role=\"presentation\"")); // button
        assert!(messages[0].text.contains("Alice"));
    }

    #[tokio::test]
    async fn sender_override() {
        let transport = Arc::new(MockTransport {
            sent: std::sync::Mutex::new(Vec::new()),
        });
        let mailer = Mailer::new(
            transport.clone(),
            Arc::new(MockTemplateProvider),
            SenderProfile {
                from_name: "Default".to_string(),
                from_email: "default@test.com".to_string(),
                reply_to: None,
            },
            Arc::new(LayoutEngine::builtin_only()),
        );

        let custom_sender = SenderProfile {
            from_name: "Tenant".to_string(),
            from_email: "tenant@custom.com".to_string(),
            reply_to: Some("support@custom.com".to_string()),
        };

        mailer
            .send(
                &SendEmail::new("welcome", "user@test.com")
                    .sender(&custom_sender)
                    .var("name", "Bob"),
            )
            .await
            .unwrap();

        let messages = transport.sent.lock().unwrap();
        assert!(messages[0].from.contains("tenant@custom.com"));
        assert_eq!(messages[0].reply_to.as_deref(), Some("support@custom.com"));
    }

    #[test]
    fn render_returns_message_without_sending() {
        let transport = Arc::new(MockTransport {
            sent: std::sync::Mutex::new(Vec::new()),
        });
        let mailer = test_mailer(transport);

        let msg = mailer
            .render(&SendEmail::new("welcome", "user@test.com").var("name", "Charlie"))
            .unwrap();

        assert_eq!(msg.subject, "Hello Charlie");
        assert!(msg.html.contains("Charlie"));
        assert!(msg.text.contains("Charlie"));
    }

    #[test]
    fn mailer_is_clone() {
        let transport = Arc::new(MockTransport {
            sent: std::sync::Mutex::new(Vec::new()),
        });
        let mailer = test_mailer(transport);
        let _clone = mailer.clone();
    }

    #[test]
    fn invalid_brand_color_falls_back_to_default() {
        let transport = Arc::new(MockTransport {
            sent: std::sync::Mutex::new(Vec::new()),
        });
        let mailer = test_mailer(transport);

        let msg = mailer
            .render(
                &SendEmail::new("welcome", "user@test.com")
                    .var("name", "Alice")
                    .var("brand_color", "red;position:absolute"),
            )
            .unwrap();

        // Should use default color, not the injection attempt
        assert!(msg.html.contains("#4F46E5"));
        assert!(!msg.html.contains("position:absolute"));
    }

    #[test]
    fn valid_brand_color_is_used() {
        let transport = Arc::new(MockTransport {
            sent: std::sync::Mutex::new(Vec::new()),
        });
        let mailer = test_mailer(transport);

        let msg = mailer
            .render(
                &SendEmail::new("welcome", "user@test.com")
                    .var("name", "Alice")
                    .var("brand_color", "#ff6600"),
            )
            .unwrap();

        assert!(msg.html.contains("#ff6600"));
    }
}
