use modo::extractor::FormReq;
use modo::{AppConfig, HandlerResult, HttpError, RequestId};

#[derive(serde::Deserialize, modo::Sanitize, modo::Validate)]
struct ContactForm {
    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    email: String,

    #[clean(trim, strip_html_tags)]
    #[validate(required, min_length = 5, max_length = 1000)]
    message: String,
}

#[modo::handler(GET, "/")]
async fn index(request_id: RequestId) -> String {
    format!("Hello modo! (request: {request_id})")
}

#[modo::handler(GET, "/health")]
async fn health() -> &'static str {
    "ok"
}

#[modo::handler(GET, "/error")]
async fn error_example() -> Result<&'static str, HttpError> {
    Err(HttpError::NotFound)
}

#[modo::handler(POST, "/contact")]
async fn contact(form: FormReq<ContactForm>) -> HandlerResult<&'static str> {
    form.validate()?;
    Ok("Thanks for your message!")
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).run().await
}
