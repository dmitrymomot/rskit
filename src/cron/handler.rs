use std::future::Future;

use crate::error::Result;

use super::context::{CronContext, FromCronContext};

pub trait CronHandler<Args>: Clone + Send + 'static {
    fn call(self, ctx: CronContext) -> impl Future<Output = Result<()>> + Send;
}

// 0 args
impl<F, Fut> CronHandler<()> for F
where
    F: FnOnce() -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    async fn call(self, _ctx: CronContext) -> Result<()> {
        (self)().await
    }
}

macro_rules! impl_cron_handler {
    ($($T:ident),+) => {
        impl<F, Fut, $($T),+> CronHandler<($($T,)+)> for F
        where
            F: FnOnce($($T),+) -> Fut + Clone + Send + 'static,
            Fut: Future<Output = Result<()>> + Send,
            $($T: FromCronContext,)+
        {
            #[allow(non_snake_case)]
            async fn call(self, ctx: CronContext) -> Result<()> {
                $(let $T = $T::from_cron_context(&ctx)?;)+
                (self)($($T),+).await
            }
        }
    };
}

impl_cron_handler!(T1);
impl_cron_handler!(T1, T2);
impl_cron_handler!(T1, T2, T3);
impl_cron_handler!(T1, T2, T3, T4);
impl_cron_handler!(T1, T2, T3, T4, T5);
impl_cron_handler!(T1, T2, T3, T4, T5, T6);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
