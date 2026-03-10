use modo::templates::{TemplateConfig, engine};

#[modo::view("pages/home.html", htmx = "partials/clock.html")]
struct HomePage {
    time: String,
    date: String,
    time_hour: u32,
}

#[modo::handler(GET, "/")]
async fn home() -> HomePage {
    let now = chrono::Local::now();
    HomePage {
        time: now.format("%H:%M:%S").to_string(),
        date: now.format("%A, %B %d, %Y").to_string(),
        time_hour: chrono::Timelike::hour(&now),
    }
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::ServerConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let template_config = TemplateConfig::default();
    let mut engine = engine(&template_config)?;

    // Production: embed templates into the binary (requires minijinja-embed).
    // #[cfg(not(debug_assertions))]
    // minijinja_embed::load_templates!(engine.env_mut());

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

    app.server_config(config).service(engine).run().await
}
