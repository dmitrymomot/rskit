use bytes::Bytes;
use http_body_util::Full;
use hyper_util::client::legacy::Client;

use super::client::RemoteBackend;
use super::memory::MemoryBackend;
use crate::error::{Error, Result};

pub(crate) enum BackendKind {
    Remote(Box<RemoteBackend>),
    #[cfg_attr(not(any(test, feature = "storage-test")), allow(dead_code))]
    Memory(MemoryBackend),
}

impl BackendKind {
    /// Returns a reference to the hyper HTTP client.
    /// Only available for the Remote backend — Memory returns an error.
    pub(crate) fn http_client(
        &self,
    ) -> Result<
        &Client<
            hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
            Full<Bytes>,
        >,
    > {
        match self {
            BackendKind::Remote(b) => Ok(b.client()),
            BackendKind::Memory(_) => {
                Err(Error::internal("URL fetch not supported in memory backend"))
            }
        }
    }
}
