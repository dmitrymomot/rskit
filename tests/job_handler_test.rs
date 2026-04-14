use modo::error::Result;
use modo::job::{JobHandler, Meta, Payload};
use modo::service::Service;

// Zero-arg handler
async fn noop_handler() -> Result<()> {
    Ok(())
}

// One-arg handler
async fn payload_handler(_payload: Payload<serde_json::Value>) -> Result<()> {
    Ok(())
}

// Two-arg handler
async fn two_arg_handler(_payload: Payload<serde_json::Value>, _meta: Meta) -> Result<()> {
    Ok(())
}

// Three-arg handler with Service
async fn full_handler(
    _payload: Payload<serde_json::Value>,
    _meta: Meta,
    _svc: Service<String>,
) -> Result<()> {
    Ok(())
}

fn assert_job_handler<H: JobHandler<Args>, Args>(_h: H) {}

#[test]
fn zero_arg_handler_satisfies_trait() {
    assert_job_handler(noop_handler);
}

#[test]
fn one_arg_handler_satisfies_trait() {
    assert_job_handler(payload_handler);
}

#[test]
fn two_arg_handler_satisfies_trait() {
    assert_job_handler(two_arg_handler);
}

#[test]
fn three_arg_handler_satisfies_trait() {
    assert_job_handler(full_handler);
}
