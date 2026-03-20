use std::future::Future;

use crate::error::Result;

use super::context::{FromJobContext, JobContext};

pub trait JobHandler<Args>: Clone + Send + 'static {
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
