use super::client::RemoteBackend;
use super::memory::MemoryBackend;

pub(crate) enum BackendKind {
    Remote(Box<RemoteBackend>),
    #[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
    Memory(MemoryBackend),
}
