use axum::Router;
use http::Method;

use crate::service::{AppState, Registry};

use super::request::TestRequestBuilder;

/// An assembled test application ready to receive in-process HTTP requests.
///
/// Obtain one via [`TestApp::builder()`] or [`TestApp::from_router()`].
/// Each HTTP-method helper returns a [`TestRequestBuilder`] that you configure
/// and then drive to completion with `.send().await`.
pub struct TestApp {
    router: Router,
}

/// Builder for [`TestApp`].
///
/// Register services, routes, layers, and merged sub-routers before calling
/// [`build()`](TestAppBuilder::build) to produce a [`TestApp`].
#[must_use]
pub struct TestAppBuilder {
    registry: Registry,
    router: Router<AppState>,
}

impl TestApp {
    /// Create a new [`TestAppBuilder`] with an empty registry and router.
    pub fn builder() -> TestAppBuilder {
        TestAppBuilder {
            registry: Registry::new(),
            router: Router::new(),
        }
    }

    /// Wrap an existing [`Router`] (already finalized with state) in a [`TestApp`].
    ///
    /// Use this when you have a fully-assembled `Router` and do not need the
    /// builder's service-registry integration.
    pub fn from_router(router: Router) -> Self {
        Self { router }
    }

    /// Start a `GET` request to `uri`.
    pub fn get(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::GET, uri)
    }

    /// Start a `POST` request to `uri`.
    pub fn post(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::POST, uri)
    }

    /// Start a `PUT` request to `uri`.
    pub fn put(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::PUT, uri)
    }

    /// Start a `PATCH` request to `uri`.
    pub fn patch(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::PATCH, uri)
    }

    /// Start a `DELETE` request to `uri`.
    pub fn delete(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::DELETE, uri)
    }

    /// Start an `OPTIONS` request to `uri`.
    pub fn options(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::OPTIONS, uri)
    }

    /// Start a request with an arbitrary HTTP `method` to `uri`.
    pub fn request(&self, method: Method, uri: &str) -> TestRequestBuilder {
        TestRequestBuilder::new(self.router.clone(), method, uri)
    }
}

impl TestAppBuilder {
    /// Register a service value of type `T` in the service registry.
    ///
    /// The value is stored by its concrete type and can be extracted in
    /// handlers via `modo::extractor::Service<T>`.
    pub fn service<T: Send + Sync + 'static>(mut self, val: T) -> Self {
        self.registry.add(val);
        self
    }

    /// Add a route to the test router.
    pub fn route(
        mut self,
        path: &str,
        method_router: axum::routing::MethodRouter<AppState>,
    ) -> Self {
        self.router = self.router.route(path, method_router);
        self
    }

    /// Apply a Tower middleware layer to the test router.
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: tower::Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: tower::Service<http::Request<axum::body::Body>> + Clone + Send + Sync + 'static,
        <L::Service as tower::Service<http::Request<axum::body::Body>>>::Response:
            axum::response::IntoResponse + 'static,
        <L::Service as tower::Service<http::Request<axum::body::Body>>>::Error:
            Into<std::convert::Infallible> + 'static,
        <L::Service as tower::Service<http::Request<axum::body::Body>>>::Future: Send + 'static,
    {
        self.router = self.router.layer(layer);
        self
    }

    /// Merge another `Router<AppState>` into the test router.
    pub fn merge(mut self, router: Router<AppState>) -> Self {
        self.router = self.router.merge(router);
        self
    }

    /// Finalize the builder: bind the service registry as router state and
    /// return a [`TestApp`] ready for sending requests.
    pub fn build(self) -> TestApp {
        let state = self.registry.into_state();
        TestApp {
            router: self.router.with_state(state),
        }
    }
}
