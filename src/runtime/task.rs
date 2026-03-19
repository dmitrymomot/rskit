use crate::error::Result;

pub trait Task: Send + 'static {
    fn shutdown(self) -> impl std::future::Future<Output = Result<()>> + Send;
}
