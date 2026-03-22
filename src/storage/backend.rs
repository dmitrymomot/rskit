use super::client::RemoteBackend;
use super::memory::MemoryBackend;

pub(crate) enum BackendKind {
    Remote(RemoteBackend),
    #[cfg_attr(not(any(test, feature = "storage-test")), allow(dead_code))]
    Memory(MemoryBackend),
}
