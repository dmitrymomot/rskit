use super::client::RemoteBackend;
use super::memory::MemoryBackend;
use crate::error::{Error, Result};

pub(crate) enum BackendKind {
    Remote(Box<RemoteBackend>),
    #[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
    Memory(MemoryBackend),
}

impl BackendKind {
    /// Returns a reference to the HTTP client.
    /// Only available for the Remote backend — Memory returns an error.
    pub(crate) fn http_client(&self) -> Result<&crate::http::Client> {
        match self {
            BackendKind::Remote(b) => Ok(b.client()),
            BackendKind::Memory(_) => {
                Err(Error::internal("URL fetch not supported in memory backend"))
            }
        }
    }
}
