use super::MailTransport;
use crate::config::ResendConfig;
use crate::message::MailMessage;

pub struct ResendTransport {
    client: reqwest::Client,
    api_key: String,
}

impl ResendTransport {
    pub fn new(config: &ResendConfig) -> Result<Self, modo::Error> {
        let client = reqwest::Client::new();
        Ok(Self {
            client,
            api_key: config.api_key.clone(),
        })
    }
}

#[async_trait::async_trait]
impl MailTransport for ResendTransport {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
        let mut body = serde_json::json!({
            "from": message.from,
            "to": [message.to],
            "subject": message.subject,
            "html": message.html,
            "text": message.text,
        });

        if let Some(ref reply_to) = message.reply_to {
            body["reply_to"] = serde_json::json!(reply_to);
        }

        let resp = self
            .client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| modo::Error::internal(format!("Resend request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(modo::Error::internal(format!(
                "Resend API error ({status}): {text}"
            )));
        }

        Ok(())
    }
}
