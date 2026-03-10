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

#[modo::template_function]
fn greeting(hour: u32) -> String {
    match hour {
        0..=11 => "Good morning".to_string(),
        12..=17 => "Good afternoon".to_string(),
        _ => "Good evening".to_string(),
    }
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).run().await
}
