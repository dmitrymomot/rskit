#![cfg(feature = "sse")]

use axum::response::IntoResponse;
use modo::sse::SseEvent;

#[tokio::test]
async fn channel_sends_events() {
    let resp = modo::sse::channel(|tx| async move {
        tx.send(SseEvent::new().event("msg").data("hello")).await?;
        tx.send(SseEvent::new().event("msg").data("world")).await?;
        Ok(())
    })
    .into_response();

    assert_eq!(resp.status(), http::StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains("data: hello"), "body: {text}");
    assert!(text.contains("data: world"), "body: {text}");
}

#[tokio::test]
async fn channel_sender_error_on_drop() {
    // When the response is dropped, the sender should get an error
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();

    let resp = modo::sse::channel(|tx| async move {
        // First send should succeed (response exists)
        let _ = tx.send(SseEvent::new().data("first")).await;
        // Wait a bit for the response to be dropped
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let send_result = tx.send(SseEvent::new().data("after-drop")).await;
        let _ = result_tx.send(send_result.is_err());
        Ok(())
    });

    // Drop the response immediately
    drop(resp);

    // The sender should eventually detect the drop
    let was_err = tokio::time::timeout(std::time::Duration::from_secs(1), result_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(was_err, "send after response drop should fail");
}
