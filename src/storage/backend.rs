use super::client::RemoteBackend;
use super::memory::MemoryBackend;

#[allow(dead_code)]
pub(crate) enum BackendKind {
    Remote(RemoteBackend),
    Memory(MemoryBackend),
}
