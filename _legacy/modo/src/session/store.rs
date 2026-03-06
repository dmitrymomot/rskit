use crate::error::Error;
use crate::session::{SessionData, SessionId, SessionMeta, SessionToken};
use std::future::Future;
use std::pin::Pin;

/// Trait for session persistence.
///
/// Implement this for your app's session backend (SQLite, Redis, etc.)
/// and register it via `app.session_store(my_store)`.
pub trait SessionStore: Send + Sync + 'static {
    fn create(
        &self,
        user_id: &str,
        meta: &SessionMeta,
    ) -> impl Future<Output = Result<SessionId, Error>> + Send;

    fn create_with(
        &self,
        user_id: &str,
        meta: &SessionMeta,
        data: serde_json::Value,
    ) -> impl Future<Output = Result<SessionId, Error>> + Send;

    fn read(
        &self,
        id: &SessionId,
    ) -> impl Future<Output = Result<Option<SessionData>, Error>> + Send;

    /// Update `last_active_at` and extend `expires_at` by `ttl` for sliding session expiry.
    fn touch(
        &self,
        id: &SessionId,
        ttl: std::time::Duration,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    fn update_data(
        &self,
        id: &SessionId,
        data: serde_json::Value,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    fn destroy(&self, id: &SessionId) -> impl Future<Output = Result<(), Error>> + Send;

    fn destroy_all_for_user(&self, user_id: &str)
    -> impl Future<Output = Result<(), Error>> + Send;

    fn read_by_token(
        &self,
        token: &SessionToken,
    ) -> impl Future<Output = Result<Option<SessionData>, Error>> + Send;

    fn update_token(
        &self,
        id: &SessionId,
        new_token: &SessionToken,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    fn destroy_all_except(
        &self,
        user_id: &str,
        except_id: &SessionId,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

/// Object-safe, type-erased version of [`SessionStore`].
///
/// This trait exists so we can store the session store as `Arc<dyn SessionStoreDyn>`
/// inside [`AppState`](crate::app::AppState). You should not need to implement this
/// directly; a blanket impl covers all `T: SessionStore`.
pub trait SessionStoreDyn: Send + Sync + 'static {
    fn create<'a>(
        &'a self,
        user_id: &'a str,
        meta: &'a SessionMeta,
    ) -> Pin<Box<dyn Future<Output = Result<SessionId, Error>> + Send + 'a>>;

    fn create_with<'a>(
        &'a self,
        user_id: &'a str,
        meta: &'a SessionMeta,
        data: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<SessionId, Error>> + Send + 'a>>;

    fn read<'a>(
        &'a self,
        id: &'a SessionId,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SessionData>, Error>> + Send + 'a>>;

    /// Update `last_active_at` and extend `expires_at` by `ttl` for sliding session expiry.
    fn touch<'a>(
        &'a self,
        id: &'a SessionId,
        ttl: std::time::Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;

    fn update_data<'a>(
        &'a self,
        id: &'a SessionId,
        data: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;

    fn destroy<'a>(
        &'a self,
        id: &'a SessionId,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;

    fn destroy_all_for_user<'a>(
        &'a self,
        user_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;

    fn read_by_token<'a>(
        &'a self,
        token: &'a SessionToken,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SessionData>, Error>> + Send + 'a>>;

    fn update_token<'a>(
        &'a self,
        id: &'a SessionId,
        new_token: &'a SessionToken,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;

    fn destroy_all_except<'a>(
        &'a self,
        user_id: &'a str,
        except_id: &'a SessionId,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;
}

/// Blanket impl: any `SessionStore` automatically implements `SessionStoreDyn`.
impl<T: SessionStore> SessionStoreDyn for T {
    fn create<'a>(
        &'a self,
        user_id: &'a str,
        meta: &'a SessionMeta,
    ) -> Pin<Box<dyn Future<Output = Result<SessionId, Error>> + Send + 'a>> {
        Box::pin(SessionStore::create(self, user_id, meta))
    }

    fn create_with<'a>(
        &'a self,
        user_id: &'a str,
        meta: &'a SessionMeta,
        data: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<SessionId, Error>> + Send + 'a>> {
        Box::pin(SessionStore::create_with(self, user_id, meta, data))
    }

    fn read<'a>(
        &'a self,
        id: &'a SessionId,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SessionData>, Error>> + Send + 'a>> {
        Box::pin(SessionStore::read(self, id))
    }

    fn touch<'a>(
        &'a self,
        id: &'a SessionId,
        ttl: std::time::Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(SessionStore::touch(self, id, ttl))
    }

    fn update_data<'a>(
        &'a self,
        id: &'a SessionId,
        data: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(SessionStore::update_data(self, id, data))
    }

    fn destroy<'a>(
        &'a self,
        id: &'a SessionId,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(SessionStore::destroy(self, id))
    }

    fn destroy_all_for_user<'a>(
        &'a self,
        user_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(SessionStore::destroy_all_for_user(self, user_id))
    }

    fn read_by_token<'a>(
        &'a self,
        token: &'a SessionToken,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SessionData>, Error>> + Send + 'a>> {
        Box::pin(SessionStore::read_by_token(self, token))
    }

    fn update_token<'a>(
        &'a self,
        id: &'a SessionId,
        new_token: &'a SessionToken,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(SessionStore::update_token(self, id, new_token))
    }

    fn destroy_all_except<'a>(
        &'a self,
        user_id: &'a str,
        except_id: &'a SessionId,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(SessionStore::destroy_all_except(self, user_id, except_id))
    }
}
