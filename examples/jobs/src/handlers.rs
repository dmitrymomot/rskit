use modo::extractor::JsonReq;
use modo::{Json, JsonResult};
use modo_jobs::JobQueue;
use serde_json::{Value, json};

use crate::jobs::{RemindJob, SayHelloJob};
use crate::payloads::{GreetingPayload, ReminderPayload};

#[modo::handler(POST, "/jobs/greet")]
async fn enqueue_greet(queue: JobQueue, input: JsonReq<GreetingPayload>) -> JsonResult<Value> {
    let job_id = SayHelloJob::enqueue(&queue, &input).await?;
    Ok(Json(json!({ "job_id": job_id.to_string() })))
}

#[modo::handler(POST, "/jobs/remind")]
async fn enqueue_remind(queue: JobQueue, input: JsonReq<ReminderPayload>) -> JsonResult<Value> {
    let run_at = chrono::Utc::now() + chrono::Duration::seconds(10);
    let job_id = RemindJob::enqueue_at(&queue, &input, run_at).await?;
    Ok(Json(
        json!({ "job_id": job_id.to_string(), "run_at": run_at.to_rfc3339() }),
    ))
}
