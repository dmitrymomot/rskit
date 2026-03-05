use crate::app::AppState;
use crate::error::RskitError;
use crate::session::SessionData;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::sync::Arc;

/// Trait for loading a user from a session user_id.
///
/// Implement this for your app's user type and register it via
/// `app.service(UserProviderService::<User>::new(my_provider))`.
///
/// # Example
/// ```rust,ignore
/// struct MyUserProvider { db: DatabaseConnection }
///
/// impl UserProvider for MyUserProvider {
///     type User = User;
///
///     async fn find_by_id(&self, id: &str) -> Result<Option<User>, RskitError> {
///         // look up user by id
///         Ok(None)
///     }
/// }
/// ```
pub trait UserProvider: Send + Sync + 'static {
    type User: Clone + Send + Sync + 'static;

    fn find_by_id(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<Option<Self::User>, RskitError>> + Send;
}

/// Object-safe, type-erased version of [`UserProvider`] for a specific user type `U`.
///
/// This trait exists so we can store the provider as `Box<dyn UserProviderFn<U>>`
/// inside [`UserProviderService`]. You should not need to implement this directly;
/// a blanket impl covers all `T: UserProvider<User = U>`.
pub trait UserProviderFn<U>: Send + Sync + 'static {
    fn find_by_id<'a>(
        &'a self,
        id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<U>, RskitError>> + Send + 'a>,
    >;
}

/// Blanket impl: any `UserProvider<User = U>` automatically implements `UserProviderFn<U>`.
impl<T, U> UserProviderFn<U> for T
where
    T: UserProvider<User = U>,
    U: Clone + Send + Sync + 'static,
{
    fn find_by_id<'a>(
        &'a self,
        id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<U>, RskitError>> + Send + 'a>,
    > {
        Box::pin(UserProvider::find_by_id(self, id))
    }
}

/// Wrapper that stores a type-erased user provider in the service registry.
///
/// Because `UserProviderService<U>` is a concrete `Sized` type (parameterized
/// only by the user type `U`), it has a stable `TypeId` and can be stored in
/// and retrieved from the [`ServiceRegistry`](crate::app::ServiceRegistry).
///
/// # Registration
/// ```rust,ignore
/// let provider = MyUserProvider { /* ... */ };
/// let app = AppBuilder::new(config)
///     .service(UserProviderService::<User>::new(provider))
///     .run()
///     .await;
/// ```
pub struct UserProviderService<U: Clone + Send + Sync + 'static> {
    inner: Box<dyn UserProviderFn<U>>,
}

impl<U: Clone + Send + Sync + 'static> UserProviderService<U> {
    /// Create a new `UserProviderService` wrapping a concrete [`UserProvider`].
    pub fn new<P: UserProvider<User = U>>(provider: P) -> Self {
        Self {
            inner: Box::new(provider),
        }
    }
}

/// Authenticated user and session data bundle.
#[derive(Debug, Clone)]
pub struct AuthData<U> {
    pub user: U,
    pub session: SessionData,
}

/// Extractor that requires authentication. Returns 401 if not authenticated.
///
/// Reads [`SessionData`] from request extensions (set by the session middleware),
/// then calls [`UserProvider::find_by_id()`] to load the user.
///
/// Requires that a [`UserProviderService<U>`] has been registered in the service
/// registry.
///
/// # Usage
/// ```rust,ignore
/// #[handler(GET, "/dashboard")]
/// async fn dashboard(auth: Auth<User>) -> impl IntoResponse {
///     let user = &auth.0.user;
///     let session = &auth.0.session;
/// }
/// ```
pub struct Auth<U: Clone + Send + Sync + 'static>(pub AuthData<U>);

/// Extractor that optionally loads the authenticated user. Never rejects.
///
/// Returns `None` if there is no session, no provider registered, or the user
/// cannot be found.
///
/// # Usage
/// ```rust,ignore
/// #[handler(GET, "/")]
/// async fn home(auth: OptionalAuth<User>) -> impl IntoResponse {
///     if let Some(auth_data) = &auth.0 {
///         // user is logged in
///     }
/// }
/// ```
pub struct OptionalAuth<U: Clone + Send + Sync + 'static>(pub Option<AuthData<U>>);

impl<U> FromRequestParts<AppState> for Auth<U>
where
    U: Clone + Send + Sync + 'static,
{
    type Rejection = RskitError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = parts
            .extensions
            .get::<SessionData>()
            .cloned()
            .ok_or(RskitError::Unauthorized)?;

        let provider: Arc<UserProviderService<U>> = state
            .services
            .get::<UserProviderService<U>>()
            .ok_or_else(|| RskitError::internal("UserProvider not registered"))?;

        let user = match provider.inner.find_by_id(&session.user_id).await? {
            Some(user) => user,
            None => {
                tracing::warn!(
                    session_id = session.id.as_str(),
                    user_id = session.user_id.as_str(),
                    "Session references nonexistent user"
                );
                if let Some(ref store) = state.session_store
                    && let Err(e) = store.destroy(&session.id).await
                {
                    tracing::error!(
                        session_id = session.id.as_str(),
                        "Failed to destroy stale session: {e}"
                    );
                }
                return Err(RskitError::Unauthorized);
            }
        };

        Ok(Auth(AuthData { user, session }))
    }
}

impl<U> FromRequestParts<AppState> for OptionalAuth<U>
where
    U: Clone + Send + Sync + 'static,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = match parts.extensions.get::<SessionData>().cloned() {
            Some(s) => s,
            None => return Ok(OptionalAuth(None)),
        };

        let provider: Arc<UserProviderService<U>> =
            match state.services.get::<UserProviderService<U>>() {
                Some(p) => p,
                None => return Ok(OptionalAuth(None)),
            };

        match provider.inner.find_by_id(&session.user_id).await {
            Ok(Some(user)) => Ok(OptionalAuth(Some(AuthData { user, session }))),
            Ok(None) => {
                // Stale session referencing deleted user — clean up like Auth does
                if let Some(ref store) = state.session_store
                    && let Err(e) = store.destroy(&session.id).await
                {
                    tracing::error!(
                        session_id = session.id.as_str(),
                        "Failed to destroy stale session: {e}"
                    );
                }
                Ok(OptionalAuth(None))
            }
            Err(e) => {
                tracing::error!(
                    session_id = session.id.as_str(),
                    user_id = session.user_id.as_str(),
                    error = %e,
                    "Failed to load user from session — treating as unauthenticated"
                );
                Ok(OptionalAuth(None))
            }
        }
    }
}
