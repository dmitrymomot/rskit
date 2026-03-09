use modo::error::HttpError;
use modo::extractors::Form;

#[derive(serde::Deserialize, modo::Sanitize, modo::Validate)]
struct ContactForm {
    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    email: String,

    #[clean(trim, strip_html)]
    #[validate(required, min_length = 5, max_length = 1000)]
    message: String,
}

#[modo::handler(GET, "/")]
async fn index(request_id: modo::RequestId) -> String {
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
async fn contact(form: Form<ContactForm>) -> Result<&'static str, modo::Error> {
    form.validate()?;
    Ok("Thanks for your message!")
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::ServerConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.server_config(config).run().await
}
