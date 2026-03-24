use std::future::Future;

use crate::error::Result;

use super::context::{FromJobContext, JobContext};

/// Trait implemented by all valid job handler functions.
///
/// A job handler is any `async fn` whose arguments each implement
/// [`FromJobContext`]. Up to 12 arguments are supported via blanket
/// implementations.
///
/// You never implement this trait directly — write a plain `async fn` and pass
/// it to [`WorkerBuilder::register`](super::worker::WorkerBuilder::register).
///
/// # Supported signatures
///
/// ```rust,no_run
/// use modo::job::{Payload, Meta};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct MyPayload { value: u32 }
///
/// // Zero arguments
/// async fn job_no_args() -> modo::Result<()> { Ok(()) }
///
/// // One argument
/// async fn job_one_arg(payload: Payload<MyPayload>) -> modo::Result<()> { Ok(()) }
///
/// // Multiple arguments (up to 12)
/// async fn job_two_args(payload: Payload<MyPayload>, meta: Meta) -> modo::Result<()> { Ok(()) }
/// ```
pub trait JobHandler<Args>: Clone + Send + 'static {
    /// Invoke the handler with extracted arguments from `ctx`.
    fn call(self, ctx: JobContext) -> impl Future<Output = Result<()>> + Send;
}

// 0 args
impl<F, Fut> JobHandler<()> for F
where
    F: FnOnce() -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    async fn call(self, _ctx: JobContext) -> Result<()> {
        (self)().await
    }
}

macro_rules! impl_job_handler {
    ($($T:ident),+) => {
        impl<F, Fut, $($T),+> JobHandler<($($T,)+)> for F
        where
            F: FnOnce($($T),+) -> Fut + Clone + Send + 'static,
            Fut: Future<Output = Result<()>> + Send,
            $($T: FromJobContext,)+
        {
            #[allow(non_snake_case)]
            async fn call(self, ctx: JobContext) -> Result<()> {
                $(let $T = $T::from_job_context(&ctx)?;)+
                (self)($($T),+).await
            }
        }
    };
}

impl_job_handler!(T1);
impl_job_handler!(T1, T2);
impl_job_handler!(T1, T2, T3);
impl_job_handler!(T1, T2, T3, T4);
impl_job_handler!(T1, T2, T3, T4, T5);
impl_job_handler!(T1, T2, T3, T4, T5, T6);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
