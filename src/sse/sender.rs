use super::event::Event;
use crate::error::Error;
use tokio::sync::mpsc;

/// Imperative event sender for [`Broadcaster::channel()`](super::Broadcaster::channel) closures.
///
/// When the client disconnects, [`send()`](Self::send) returns an error.
/// Use this as a signal to stop producing events.
pub struct Sender {
    pub(super) tx: mpsc::Sender<Event>,
}

impl Sender {
    /// Send an event to the connected client.
    ///
    /// # Errors
    ///
    /// Returns an error if the client has disconnected (the response stream
    /// was dropped).
    pub async fn send(&self, event: Event) -> Result<(), Error> {
        self.tx
            .send(event)
            .await
            .map_err(|_| Error::internal("SSE client disconnected"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn send_delivers_event() {
        let (tx, mut rx) = mpsc::channel(16);
        let sender = Sender { tx };
        let event = super::super::Event::new("id1", "test")
            .unwrap()
            .data("hello");
        sender.send(event).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, "id1");
        assert_eq!(received.event, "test");
        assert_eq!(received.data.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn send_returns_error_when_receiver_dropped() {
        let (tx, rx) = mpsc::channel(16);
        let sender = Sender { tx };
        drop(rx);

        let event = super::super::Event::new("id1", "test")
            .unwrap()
            .data("hello");
        let result = sender.send(event).await;
        assert!(result.is_err());
    }
}
