use std::sync::Arc;

use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::Error;

use super::state::{FlashEntry, FlashState};

pub struct Flash {
    state: Arc<FlashState>,
}

impl Flash {
    pub fn set(&self, level: &str, message: &str) {
        self.state.push(level, message);
    }

    pub fn success(&self, message: &str) {
        self.set("success", message);
    }

    pub fn error(&self, message: &str) {
        self.set("error", message);
    }

    pub fn warning(&self, message: &str) {
        self.set("warning", message);
    }

    pub fn info(&self, message: &str) {
        self.set("info", message);
    }

    /// Read incoming flash messages and mark as read.
    /// After calling this, the middleware will clear the flash cookie on response.
    /// Returns the same data on repeated calls within the same request.
    pub fn messages(&self) -> Vec<FlashEntry> {
        self.state.mark_read();
        self.state.incoming.clone()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Flash {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Arc<FlashState>>()
            .cloned()
            .map(|state| Flash { state })
            .ok_or_else(|| Error::internal("flash middleware not applied"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn set_pushes_to_outgoing() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash {
            state: state.clone(),
        };
        flash.set("custom", "hello");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].level, "custom");
        assert_eq!(outgoing[0].message, "hello");
    }

    #[test]
    fn success_uses_correct_level() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash {
            state: state.clone(),
        };
        flash.success("done");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing[0].level, "success");
    }

    #[test]
    fn error_uses_correct_level() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash {
            state: state.clone(),
        };
        flash.error("fail");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing[0].level, "error");
    }

    #[test]
    fn warning_uses_correct_level() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash {
            state: state.clone(),
        };
        flash.warning("careful");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing[0].level, "warning");
    }

    #[test]
    fn info_uses_correct_level() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash {
            state: state.clone(),
        };
        flash.info("fyi");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing[0].level, "info");
    }

    #[test]
    fn multiple_messages_preserved() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash {
            state: state.clone(),
        };
        flash.success("one");
        flash.error("two");
        flash.info("three");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing.len(), 3);
    }

    #[test]
    fn messages_returns_incoming_and_marks_read() {
        let entries = vec![
            FlashEntry {
                level: "success".into(),
                message: "saved".into(),
            },
            FlashEntry {
                level: "error".into(),
                message: "oops".into(),
            },
        ];
        let state = Arc::new(FlashState::new(entries.clone()));
        let flash = Flash {
            state: state.clone(),
        };

        let msgs = flash.messages();
        assert_eq!(msgs, entries);
        assert!(state.was_read());
    }

    #[test]
    fn messages_returns_empty_when_no_incoming() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash {
            state: state.clone(),
        };

        let msgs = flash.messages();
        assert!(msgs.is_empty());
        assert!(state.was_read());
    }

    #[test]
    fn messages_idempotent() {
        let entries = vec![FlashEntry {
            level: "info".into(),
            message: "hi".into(),
        }];
        let state = Arc::new(FlashState::new(entries.clone()));
        let flash = Flash {
            state: state.clone(),
        };

        let first = flash.messages();
        let second = flash.messages();
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn extract_from_extensions() {
        let state = Arc::new(FlashState::new(vec![]));
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(state.clone());

        let result = <Flash as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        let flash = result.unwrap();
        flash.success("test");
        assert_eq!(state.drain_outgoing().len(), 1);
    }

    #[tokio::test]
    async fn extract_missing_returns_500() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result = <Flash as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
