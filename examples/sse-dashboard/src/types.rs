use modo::sse::SseBroadcastManager;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ServerStatus {
    pub(crate) name: String,
    pub(crate) status: String, // "up", "down", "degraded"
    pub(crate) cpu: u32,
    pub(crate) memory: u32,
    pub(crate) latency_ms: u32,
}

pub(crate) type StatusBroadcaster = SseBroadcastManager<(), Vec<ServerStatus>>;
