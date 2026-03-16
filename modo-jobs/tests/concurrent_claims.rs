mod common;

use common::setup_db;
use modo_db::sea_orm::{ActiveModelTrait, ActiveValue};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::runner;
use modo_jobs::{JobId, JobState};
use std::collections::HashSet;
use std::sync::Arc;

async fn insert_pending_job(db: &modo_db::sea_orm::DatabaseConnection, id: &JobId) {
    let now = chrono::Utc::now();
    let model = jobs_entity::ActiveModel {
        id: ActiveValue::Set(id.as_str().to_string()),
        name: ActiveValue::Set("test_job".to_string()),
        queue: ActiveValue::Set("default".to_string()),
        payload: ActiveValue::Set("{}".to_string()),
        state: ActiveValue::Set(JobState::Pending.as_str().to_string()),
        priority: ActiveValue::Set(0),
        attempts: ActiveValue::Set(0),
        max_attempts: ActiveValue::Set(3),
        run_at: ActiveValue::Set(now),
        timeout_secs: ActiveValue::Set(300),
        locked_by: ActiveValue::Set(None),
        locked_at: ActiveValue::Set(None),
        last_error: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    model.insert(db).await.expect("Insert failed");
}

#[tokio::test]
async fn no_double_claims_under_concurrency() {
    let db = setup_db().await;

    // Insert 10 pending jobs
    let mut job_ids: HashSet<String> = HashSet::new();
    for _ in 0..10 {
        let id = JobId::new();
        job_ids.insert(id.as_str().to_string());
        insert_pending_job(&db, &id).await;
    }

    // Spawn 20 workers (2x the number of jobs) each trying to claim
    let db = Arc::new(db);
    let mut handles = Vec::new();

    for worker_num in 0..20 {
        let db = db.clone();
        let worker_id = format!("worker-{worker_num}");
        handles.push(tokio::spawn(async move {
            runner::claim_next(&db, "default", &worker_id)
                .await
                .expect("Claim failed")
                .map(|job| job.id)
        }));
    }

    // Collect all claimed job IDs
    let mut claimed_ids: Vec<String> = Vec::new();
    for handle in handles {
        if let Some(id) = handle.await.expect("Task panicked") {
            claimed_ids.push(id);
        }
    }

    // Verify: exactly 10 jobs claimed (all jobs were claimed)
    assert_eq!(
        claimed_ids.len(),
        10,
        "Should have claimed exactly 10 jobs, got {}",
        claimed_ids.len()
    );

    // Verify: no duplicates
    let claimed_set: HashSet<String> = claimed_ids.iter().cloned().collect();
    assert_eq!(
        claimed_set.len(),
        claimed_ids.len(),
        "No duplicate claims should occur"
    );

    // Verify: all claimed IDs are from the original set
    for claimed_id in &claimed_set {
        assert!(
            job_ids.contains(claimed_id),
            "Claimed ID {claimed_id} should be from the original job set"
        );
    }
}
