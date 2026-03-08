use modo_templates::{TemplateConfig, engine};

#[modo::view("pages/home.html", htmx = "partials/clock.html")]
struct HomePage {
    time: String,
    date: String,
}

#[modo::handler(GET, "/")]
async fn home() -> HomePage {
    let now = chrono::Local::now();
    HomePage {
        time: now.format("%H:%M:%S").to_string(),
        date: now.format("%A, %B %d, %Y").to_string(),
    }
}

#[modo::main]
async fn main(app: modo::app::AppBuilder) -> Result<(), Box<dyn std::error::Error>> {
    let config = TemplateConfig::default();
    let mut engine = engine(&config)?;

    // Custom template function — demonstrates env_mut().add_function() API
    engine
        .env_mut()
        .add_function("greeting", |hour: u32| -> String {
            match hour {
                0..=11 => "Good morning".to_string(),
                12..=17 => "Good afternoon".to_string(),
                _ => "Good evening".to_string(),
            }
        });

    app.security_headers(modo::SecurityHeadersConfig {
        content_security_policy: Some(
            "default-src 'self'; script-src 'self' https://unpkg.com; style-src 'self' 'unsafe-inline'".to_string(),
        ),
        ..Default::default()
    })
    .service(engine)
    .run()
    .await
}
