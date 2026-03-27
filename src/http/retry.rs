use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;

use super::client::InnerHttpsClient;
use super::response::Response;
use crate::error::{Error, Result};

/// Maximum duration honoured from a `Retry-After` header.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

/// Retry configuration for a single request.
pub(crate) struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries).
    pub max_retries: u32,
    /// Base backoff duration; actual delay is `backoff * 2^attempt`.
    pub backoff: Duration,
}

/// Classification of a single send attempt.
enum Attempt {
    /// Received a successful (or non-retryable) response.
    Success(Response),
    /// Got an HTTP response with a retryable status (502, 503, 429).
    RetryableResponse(Response, String, Option<Duration>),
    /// Transient connection/timeout failure that can be retried.
    RetryableError(String, Option<Duration>),
    /// Fatal error that must not be retried.
    Fatal(Error),
}

/// Classification of a low-level send error.
enum SendError {
    Timeout,
    Connection(String),
    Fatal(String),
}

/// Execute a request with the configured retry policy.
///
/// The `build_request` closure is called once per attempt so that the request body
/// (which is consumed by hyper) is recreated each time.
pub(crate) async fn execute<F>(
    client: &InnerHttpsClient,
    policy: &RetryPolicy,
    url: &str,
    timeout: Option<Duration>,
    build_request: F,
) -> Result<Response>
where
    F: Fn() -> Result<hyper::Request<Full<Bytes>>>,
{
    let mut last_retryable_response: Option<Response> = None;
    let mut last_err: Option<Error> = None;

    for attempt in 0..=policy.max_retries {
        let request = build_request()?;
        let method_str = request.method().to_string();

        tracing::debug!(attempt, url = %url, method = %method_str, "http.request");

        match classify(send_once(client, request, url, timeout).await) {
            Attempt::Success(resp) => return Ok(resp),
            Attempt::Fatal(e) => return Err(e),
            Attempt::RetryableResponse(resp, reason, retry_after) => {
                // Don't sleep after the final attempt.
                if attempt < policy.max_retries {
                    let delay =
                        retry_after.unwrap_or_else(|| backoff_delay(policy.backoff, attempt));
                    tracing::warn!(
                        attempt,
                        next_attempt = attempt + 1,
                        reason = %reason,
                        backoff_ms = delay.as_millis() as u64,
                        "http.retry"
                    );
                    // Store before sleeping so we have it if we exhaust retries.
                    last_retryable_response = Some(resp);
                    tokio::time::sleep(delay).await;
                } else {
                    // Final attempt with retryable HTTP status — return the response.
                    last_retryable_response = Some(resp);
                }
            }
            Attempt::RetryableError(reason, retry_after) => {
                last_err = Some(Error::internal(reason.clone()));
                last_retryable_response = None;

                // Don't sleep after the final attempt.
                if attempt < policy.max_retries {
                    let delay =
                        retry_after.unwrap_or_else(|| backoff_delay(policy.backoff, attempt));
                    tracing::warn!(
                        attempt,
                        next_attempt = attempt + 1,
                        reason = %reason,
                        backoff_ms = delay.as_millis() as u64,
                        "http.retry"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    // Exhausted retries: if the last attempt was a retryable HTTP response, return it.
    if let Some(resp) = last_retryable_response {
        return Ok(resp);
    }

    Err(last_err.unwrap_or_else(|| Error::internal("HTTP request failed after retries")))
}

/// Send a single HTTP request, applying the optional timeout.
async fn send_once(
    client: &InnerHttpsClient,
    request: hyper::Request<Full<Bytes>>,
    url: &str,
    timeout: Option<Duration>,
) -> std::result::Result<Response, SendError> {
    let fut = client.request(request);

    let hyper_resp = if let Some(dur) = timeout {
        match tokio::time::timeout(dur, fut).await {
            Ok(result) => result.map_err(classify_hyper_error)?,
            Err(_) => return Err(SendError::Timeout),
        }
    } else {
        fut.await.map_err(classify_hyper_error)?
    };

    let status = hyper_resp.status();
    let headers = hyper_resp.headers().clone();
    let body = hyper_resp.into_body();

    Ok(Response::new(status, headers, url.to_string(), body))
}

/// Classify a hyper client error as connection or fatal.
fn classify_hyper_error(e: hyper_util::client::legacy::Error) -> SendError {
    let msg = e.to_string();
    if e.is_connect() || msg.contains("connection") || msg.contains("dns") {
        SendError::Connection(msg)
    } else {
        SendError::Fatal(msg)
    }
}

/// Classify an attempt result into success, retryable, or fatal.
fn classify(result: std::result::Result<Response, SendError>) -> Attempt {
    match result {
        Ok(resp) => {
            let status = resp.status();
            if status == http::StatusCode::TOO_MANY_REQUESTS {
                let retry_after = resp
                    .headers()
                    .get(http::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after);
                Attempt::RetryableResponse(resp, format!("HTTP {status}"), retry_after)
            } else if status == http::StatusCode::BAD_GATEWAY
                || status == http::StatusCode::SERVICE_UNAVAILABLE
            {
                Attempt::RetryableResponse(resp, format!("HTTP {status}"), None)
            } else {
                Attempt::Success(resp)
            }
        }
        Err(SendError::Timeout) => Attempt::RetryableError("request timed out".into(), None),
        Err(SendError::Connection(msg)) => Attempt::RetryableError(msg, None),
        Err(SendError::Fatal(msg)) => {
            Attempt::Fatal(Error::internal(format!("HTTP request failed: {msg}")))
        }
    }
}

/// Parse a `Retry-After` header value (seconds or HTTP-date), capped at [`MAX_RETRY_AFTER`].
fn parse_retry_after(value: &str) -> Option<Duration> {
    // Try integer seconds first.
    if let Ok(secs) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(secs).min(MAX_RETRY_AFTER));
    }

    // Try HTTP-date (RFC 7231 section 7.1.1.1).
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(value.trim()) {
        let now = chrono::Utc::now();
        let target = date.with_timezone(&chrono::Utc);
        if target > now {
            let delta = (target - now).to_std().ok()?;
            return Some(delta.min(MAX_RETRY_AFTER));
        }
        // Date is in the past — retry immediately.
        return Some(Duration::ZERO);
    }

    None
}

/// Compute exponential backoff: `base * 2^attempt`.
fn backoff_delay(base: Duration, attempt: u32) -> Duration {
    base.saturating_mul(2u32.saturating_pow(attempt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_exponential() {
        let base = Duration::from_millis(100);
        assert_eq!(backoff_delay(base, 0), Duration::from_millis(100));
        assert_eq!(backoff_delay(base, 1), Duration::from_millis(200));
        assert_eq!(backoff_delay(base, 2), Duration::from_millis(400));
        assert_eq!(backoff_delay(base, 3), Duration::from_millis(800));
    }

    #[test]
    fn backoff_saturates() {
        let base = Duration::from_secs(1);
        // Very large exponent should saturate rather than overflow.
        let delay = backoff_delay(base, 100);
        assert!(delay >= Duration::from_secs(1));
    }

    #[test]
    fn parse_retry_after_seconds() {
        assert_eq!(parse_retry_after("5"), Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_retry_after_caps_at_max() {
        assert_eq!(parse_retry_after("3600"), Some(MAX_RETRY_AFTER));
    }

    #[test]
    fn parse_retry_after_invalid() {
        assert_eq!(parse_retry_after("not-a-number"), None);
    }

    #[test]
    fn parse_retry_after_zero() {
        assert_eq!(parse_retry_after("0"), Some(Duration::ZERO));
    }
}
