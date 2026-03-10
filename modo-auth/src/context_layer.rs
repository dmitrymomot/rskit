use crate::cache::ResolvedUser;
use crate::provider::UserProviderService;

use futures_util::future::BoxFuture;
use modo::axum::http::Request;
use modo::templates::TemplateContext;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Layer that injects the authenticated user into TemplateContext.
/// Graceful: injects nothing if not authenticated.
pub struct UserContextLayer<U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    user_svc: UserProviderService<U>,
}

impl<U> Clone for UserContextLayer<U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    fn clone(&self) -> Self {
        Self {
            user_svc: self.user_svc.clone(),
        }
    }
}

impl<U> UserContextLayer<U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    pub fn new(user_svc: UserProviderService<U>) -> Self {
        Self { user_svc }
    }
}

impl<S, U> Layer<S> for UserContextLayer<U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Service = UserContextMiddleware<S, U>;

    fn layer(&self, inner: S) -> Self::Service {
        UserContextMiddleware {
            inner,
            user_svc: self.user_svc.clone(),
        }
    }
}

#[derive(Clone)]
pub struct UserContextMiddleware<S, U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    inner: S,
    user_svc: UserProviderService<U>,
}

impl<S, ReqBody, ResBody, U> Service<Request<ReqBody>> for UserContextMiddleware<S, U>
where
    S: Service<Request<ReqBody>, Response = modo::axum::http::Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        let user_svc = self.user_svc.clone();

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Try to get user_id from session extensions
            let user_id = modo_session::user_id_from_extensions(&parts.extensions);

            if let Some(user_id) = user_id
                && let Ok(Some(user)) = user_svc.find_by_id(&user_id).await
            {
                if let Some(ctx) = parts.extensions.get_mut::<TemplateContext>() {
                    ctx.insert("user", modo::minijinja::Value::from_serialize(&user));
                }
                parts.extensions.insert(ResolvedUser(Arc::new(user)));
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}
