use modo::extractors::service::Service;
use modo::sse::{Sse, SseEvent, SseResponse, SseStreamExt};
use modo::templates::ViewRenderer;

use crate::types::{ServerStatus, StatusBroadcaster};

#[modo::view("pages/dashboard.html")]
struct DashboardPage {}

#[modo::handler(GET, "/")]
async fn dashboard() -> DashboardPage {
    DashboardPage {}
}

#[modo::handler(GET, "/events")]
async fn events(
    sse: Sse,
    view: ViewRenderer,
    Service(bc): Service<StatusBroadcaster>,
) -> SseResponse {
    let stream = bc.subscribe(&()).sse_map(move |servers| {
        let html = view.render_to_string(StatusCards { servers })?;
        Ok(SseEvent::new().event("status_update").html(html))
    });
    sse.from_stream(stream)
}

#[modo::view("partials/status_card.html")]
struct StatusCards {
    servers: Vec<ServerStatus>,
}
