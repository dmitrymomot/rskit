use axum::Router;
use http::Method;

use crate::service::{AppState, Registry};

use super::request::TestRequestBuilder;

pub struct TestApp {
    router: Router,
}

pub struct TestAppBuilder {
    registry: Registry,
    router: Router<AppState>,
}

impl TestApp {
    pub fn builder() -> TestAppBuilder {
        TestAppBuilder {
            registry: Registry::new(),
            router: Router::new(),
        }
    }

    pub fn from_router(router: Router) -> Self {
        Self { router }
    }

    pub fn get(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::GET, uri)
    }

    pub fn post(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::POST, uri)
    }

    pub fn put(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::PUT, uri)
    }

    pub fn patch(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::PATCH, uri)
    }

    pub fn delete(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::DELETE, uri)
    }

    pub fn options(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::OPTIONS, uri)
    }

    pub fn request(&self, method: Method, uri: &str) -> TestRequestBuilder {
        TestRequestBuilder::new(self.router.clone(), method, uri)
    }
}

impl TestAppBuilder {
    pub fn service<T: Send + Sync + 'static>(mut self, val: T) -> Self {
        self.registry.add(val);
        self
    }

    pub fn route(
        mut self,
        path: &str,
        method_router: axum::routing::MethodRouter<AppState>,
    ) -> Self {
        self.router = self.router.route(path, method_router);
        self
    }

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

    pub fn merge(mut self, router: Router<AppState>) -> Self {
        self.router = self.router.merge(router);
        self
    }

    pub fn build(self) -> TestApp {
        let state = self.registry.into_state();
        TestApp {
            router: self.router.with_state(state),
        }
    }
}
