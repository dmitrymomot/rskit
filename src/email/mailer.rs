use crate::email::cache::CachedSource;
use crate::email::layout;
use crate::email::markdown;
use crate::email::message::{RenderedEmail, SendEmail};
use crate::email::render;
use crate::email::source::{FileSource, TemplateSource};
use crate::{Error, Result};
use lettre::message::{MultiPart, SinglePart, header::ContentType};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::collections::HashMap;
use std::sync::Arc;

use crate::email::config::{EmailConfig, SmtpSecurity};

enum Transport {
    Smtp(AsyncSmtpTransport<Tokio1Executor>),
    #[cfg(feature = "email-test")]
    Stub(lettre::transport::stub::AsyncStubTransport),
}

pub struct Mailer {
    source: Arc<dyn TemplateSource>,
    transport: Transport,
    config: EmailConfig,
    layouts: HashMap<String, String>,
}

impl Mailer {
    /// Create a new Mailer with the default FileSource (cached if configured).
    pub fn new(config: &EmailConfig) -> Result<Self> {
        let file_source = FileSource::new(&config.templates_path);
        let source: Arc<dyn TemplateSource> = if config.cache_templates {
            Arc::new(CachedSource::new(file_source, config.template_cache_size))
        } else {
            Arc::new(file_source)
        };

        let transport = Self::build_smtp_transport(config)?;
        let layouts = layout::load_layouts(&config.layouts_path)?;

        Ok(Self {
            source,
            transport: Transport::Smtp(transport),
            config: config.clone(),
            layouts,
        })
    }

    /// Create a new Mailer with a custom TemplateSource.
    pub fn with_source(config: &EmailConfig, source: Arc<dyn TemplateSource>) -> Result<Self> {
        let transport = Self::build_smtp_transport(config)?;
        let layouts = layout::load_layouts(&config.layouts_path)?;

        Ok(Self {
            source,
            transport: Transport::Smtp(transport),
            config: config.clone(),
            layouts,
        })
    }

    /// Create a Mailer with a stub transport for testing.
    #[cfg(feature = "email-test")]
    pub fn with_stub_transport(
        config: &EmailConfig,
        stub: lettre::transport::stub::AsyncStubTransport,
    ) -> Result<Self> {
        let file_source = FileSource::new(&config.templates_path);
        let source: Arc<dyn TemplateSource> = if config.cache_templates {
            Arc::new(CachedSource::new(file_source, config.template_cache_size))
        } else {
            Arc::new(file_source)
        };
        let layouts = layout::load_layouts(&config.layouts_path)?;

        Ok(Self {
            source,
            transport: Transport::Stub(stub),
            config: config.clone(),
            layouts,
        })
    }

    fn build_smtp_transport(config: &EmailConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
        // Validate SMTP auth: both set or both empty
        match (&config.smtp.username, &config.smtp.password) {
            (Some(_), None) | (None, Some(_)) => {
                return Err(Error::bad_request(
                    "SMTP username and password must both be set or both be empty",
                ));
            }
            _ => {}
        }

        let builder = match config.smtp.security {
            SmtpSecurity::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp.host)
                .map_err(|e| Error::internal(format!("SMTP relay error: {e}")))?,
            SmtpSecurity::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp.host)
                    .map_err(|e| Error::internal(format!("SMTP STARTTLS error: {e}")))?
            }
            SmtpSecurity::None => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp.host)
            }
        };

        let builder = builder.port(config.smtp.port);

        let builder = if let (Some(username), Some(password)) =
            (&config.smtp.username, &config.smtp.password)
        {
            builder.credentials(lettre::transport::smtp::authentication::Credentials::new(
                username.clone(),
                password.clone(),
            ))
        } else {
            builder
        };

        Ok(builder.build())
    }

    /// Render a template without sending.
    pub fn render(&self, email: &SendEmail) -> Result<RenderedEmail> {
        let locale = email
            .locale
            .as_deref()
            .unwrap_or(&self.config.default_locale);

        // Load raw template
        let raw = self
            .source
            .load(&email.template, locale, &self.config.default_locale)?;

        // Stage 1: Substitute variables
        let substituted = render::substitute(&raw, &email.vars);

        // Stage 2: Parse frontmatter
        let (frontmatter, body) = render::parse_frontmatter(&substituted)?;

        // Stage 3: Render markdown to HTML
        let brand_color = email.vars.get("brand_color").map(|s| s.as_str());
        let html_body = markdown::markdown_to_html(body, brand_color);

        // Stage 4: Apply layout
        let layout_html = layout::resolve_layout(&frontmatter.layout, &self.layouts)?;
        let html = layout::apply_layout(&layout_html, &html_body, &email.vars);

        // Stage 5: Plain text
        let text = markdown::markdown_to_text(body);

        Ok(RenderedEmail {
            subject: frontmatter.subject,
            html,
            text,
        })
    }

    /// Render and send an email via SMTP.
    pub async fn send(&self, email: SendEmail) -> Result<()> {
        if email.to.is_empty() {
            return Err(Error::bad_request("email has no recipients"));
        }

        let rendered = self.render(&email)?;

        // Build sender
        let from_name = email
            .sender
            .as_ref()
            .map(|s| &s.from_name)
            .unwrap_or(&self.config.default_from_name);
        let from_email = email
            .sender
            .as_ref()
            .map(|s| &s.from_email)
            .unwrap_or(&self.config.default_from_email);
        let reply_to = email
            .sender
            .as_ref()
            .and_then(|s| s.reply_to.as_deref())
            .or(self.config.default_reply_to.as_deref());

        let from = if from_name.is_empty() {
            from_email.parse()
        } else {
            format!("{from_name} <{from_email}>").parse()
        }
        .map_err(|e| Error::bad_request(format!("invalid from address: {e}")))?;

        let mut builder = Message::builder().from(from).subject(&rendered.subject);

        for to_addr in &email.to {
            builder = builder.to(to_addr
                .parse()
                .map_err(|e| Error::bad_request(format!("invalid to address '{to_addr}': {e}")))?);
        }

        for cc_addr in &email.cc {
            builder = builder.cc(cc_addr
                .parse()
                .map_err(|e| Error::bad_request(format!("invalid cc address '{cc_addr}': {e}")))?);
        }

        for bcc_addr in &email.bcc {
            builder = builder.bcc(bcc_addr.parse().map_err(|e| {
                Error::bad_request(format!("invalid bcc address '{bcc_addr}': {e}"))
            })?);
        }

        if let Some(reply_to_addr) = reply_to {
            builder = builder.reply_to(
                reply_to_addr
                    .parse()
                    .map_err(|e| Error::bad_request(format!("invalid reply-to address: {e}")))?,
            );
        }

        let message = builder
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(rendered.text),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(rendered.html),
                    ),
            )
            .map_err(|e| Error::internal(format!("failed to build email message: {e}")))?;

        match &self.transport {
            Transport::Smtp(transport) => {
                transport
                    .send(message)
                    .await
                    .map_err(|e| Error::internal(format!("failed to send email: {e}")))?;
            }
            #[cfg(feature = "email-test")]
            Transport::Stub(transport) => {
                transport
                    .send(message)
                    .await
                    .map_err(|e| Error::internal(format!("failed to send email (stub): {e}")))?;
            }
        }

        Ok(())
    }
}
