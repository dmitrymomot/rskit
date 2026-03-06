use modo::error::Error;

#[modo::handler(GET, "/")]
async fn index() -> &'static str {
    "Hello modo!"
}

#[modo::handler(GET, "/health")]
async fn health() -> &'static str {
    "ok"
}

#[modo::handler(GET, "/error")]
async fn error_example() -> Result<&'static str, Error> {
    Err(Error::NotFound)
}

#[modo::main]
async fn main(app: modo::app::AppBuilder) -> Result<(), Box<dyn std::error::Error>> {
    app.run().await
}
