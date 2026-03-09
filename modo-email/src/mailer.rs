use crate::message::{MailMessage, SendEmail, SenderProfile};
use crate::template::layout::LayoutEngine;
use crate::template::{markdown, vars, TemplateProvider};
use crate::transport::MailTransport;
use std::collections::HashMap;

/// High-level email service that ties together template loading, variable
/// substitution, Markdown rendering, layout wrapping, and transport delivery.
pub struct Mailer {
    transport: Box<dyn MailTransport>,
    templates: Box<dyn TemplateProvider>,
    default_sender: SenderProfile,
    layout_engine: LayoutEngine,
}

impl Mailer {
    pub fn new(
        transport: Box<dyn MailTransport>,
        templates: Box<dyn TemplateProvider>,
        default_sender: SenderProfile,
        layout_engine: LayoutEngine,
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

        // Render Markdown body to HTML (with optional custom button color).
        let button_color = email
            .context
            .get("brand_color")
            .and_then(|v| v.as_str())
            .unwrap_or("#4F46E5");
        let html_body = markdown::render_markdown_with_color(&body, button_color);
        let text = markdown::render_plain_text(&body);

        // Wrap HTML body in a layout.
        let layout_name = template.layout.as_deref().unwrap_or("default");
        let mut layout_ctx: HashMap<String, serde_json::Value> = email.context.clone();
        layout_ctx.insert(
            "content".to_string(),
            serde_json::Value::String(html_body),
        );
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
    pub async fn send(&self, email: SendEmail) -> Result<(), modo::Error> {
        let message = self.render(&email)?;
        self.transport.send(&message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::EmailTemplate;
    use std::sync::{Arc, Mutex};

    struct MockTransport {
        sent: Arc<Mutex<Vec<MailMessage>>>,
    }

    #[async_trait::async_trait]
    impl MailTransport for MockTransport {
        async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
            self.sent.lock().unwrap().push(MailMessage {
                from: message.from.clone(),
                reply_to: message.reply_to.clone(),
                to: message.to.clone(),
                subject: message.subject.clone(),
                html: message.html.clone(),
                text: message.text.clone(),
            });
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

    #[tokio::test]
    async fn send_renders_and_delivers() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mailer = Mailer::new(
            Box::new(MockTransport { sent: sent.clone() }),
            Box::new(MockTemplateProvider),
            SenderProfile {
                from_name: "Test".to_string(),
                from_email: "test@test.com".to_string(),
                reply_to: None,
            },
            LayoutEngine::builtin_only(),
        );

        mailer
            .send(SendEmail::new("welcome", "user@test.com").var("name", "Alice"))
            .await
            .unwrap();

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].to, "user@test.com");
        assert_eq!(messages[0].subject, "Hello Alice");
        assert!(messages[0].html.contains("Alice"));
        assert!(messages[0].html.contains("role=\"presentation\"")); // button
        assert!(messages[0].text.contains("Alice"));
    }

    #[tokio::test]
    async fn sender_override() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mailer = Mailer::new(
            Box::new(MockTransport { sent: sent.clone() }),
            Box::new(MockTemplateProvider),
            SenderProfile {
                from_name: "Default".to_string(),
                from_email: "default@test.com".to_string(),
                reply_to: None,
            },
            LayoutEngine::builtin_only(),
        );

        let custom_sender = SenderProfile {
            from_name: "Tenant".to_string(),
            from_email: "tenant@custom.com".to_string(),
            reply_to: Some("support@custom.com".to_string()),
        };

        mailer
            .send(
                SendEmail::new("welcome", "user@test.com")
                    .sender(&custom_sender)
                    .var("name", "Bob"),
            )
            .await
            .unwrap();

        let messages = sent.lock().unwrap();
        assert!(messages[0].from.contains("tenant@custom.com"));
        assert_eq!(messages[0].reply_to.as_deref(), Some("support@custom.com"));
    }

    #[test]
    fn render_returns_message_without_sending() {
        let mailer = Mailer::new(
            Box::new(MockTransport {
                sent: Arc::new(Mutex::new(Vec::new())),
            }),
            Box::new(MockTemplateProvider),
            SenderProfile {
                from_name: "Test".to_string(),
                from_email: "test@test.com".to_string(),
                reply_to: None,
            },
            LayoutEngine::builtin_only(),
        );

        let msg = mailer
            .render(&SendEmail::new("welcome", "user@test.com").var("name", "Charlie"))
            .unwrap();

        assert_eq!(msg.subject, "Hello Charlie");
        assert!(msg.html.contains("Charlie"));
        assert!(msg.text.contains("Charlie"));
    }
}
