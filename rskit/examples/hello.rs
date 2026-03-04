use rskit::error::RskitError;

#[rskit::handler(GET, "/")]
async fn index() -> &'static str {
    "Hello rskit!"
}

#[rskit::handler(GET, "/health")]
async fn health() -> &'static str {
    "ok"
}

#[rskit::handler(GET, "/error")]
async fn error_example() -> Result<&'static str, RskitError> {
    Err(RskitError::NotFound)
}

#[rskit::main]
async fn main(app: rskit::app::AppBuilder) -> Result<(), Box<dyn std::error::Error>> {
    app.run().await
}
