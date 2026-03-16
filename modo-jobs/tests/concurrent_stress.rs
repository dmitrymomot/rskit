mod common;

use chrono::Utc;
use common::setup_db;
use modo_db::sea_orm::{ActiveModelTrait, ActiveValue};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::runner;
use modo_jobs::{JobId, JobState};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

async fn insert_job(db: &modo_db::sea_orm::DatabaseConnection, id: &JobId) {
    let now = Utc::now();
    let model = jobs_entity::ActiveModel {
        id: ActiveValue::Set(id.as_str().to_string()),
        name: ActiveValue::Set("stress_job".to_string()),
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
async fn concurrent_inserts_and_claims() {
    let db = Arc::new(setup_db().await);

    // Phase 1: 50 concurrent inserts
    let mut insert_set = JoinSet::new();
    let mut expected_ids = HashSet::new();

    for _ in 0..50 {
        let id = JobId::new();
        expected_ids.insert(id.as_str().to_string());
        let db = db.clone();
        insert_set.spawn(async move {
            insert_job(&*db, &id).await;
        });
    }

    // Wait for all inserts to complete
    while let Some(result) = insert_set.join_next().await {
        result.expect("Insert task panicked");
    }

    // Phase 2: 10 workers claiming until queue is empty
    let claimed_ids = Arc::new(Mutex::new(Vec::new()));
    let mut claim_set = JoinSet::new();

    for worker_num in 0..10 {
        let db = db.clone();
        let claimed_ids = claimed_ids.clone();
        let worker_id = format!("worker-{worker_num}");

        claim_set.spawn(async move {
            loop {
                let result = runner::claim_next(&*db, "default", &worker_id)
                    .await
                    .expect("Claim failed");

                match result {
                    Some(job) => {
                        let mut ids = claimed_ids.lock().await;
                        ids.push(job.id);
                    }
                    None => break, // No more jobs
                }
            }
        });
    }

    // Wait for all workers to finish
    while let Some(result) = claim_set.join_next().await {
        result.expect("Claim task panicked");
    }

    let claimed = claimed_ids.lock().await;

    // Verify: total claimed == 50
    assert_eq!(
        claimed.len(),
        50,
        "Expected 50 claimed jobs, got {}",
        claimed.len()
    );

    // Verify: no duplicate claims
    let claimed_set: HashSet<String> = claimed.iter().cloned().collect();
    assert_eq!(
        claimed_set.len(),
        claimed.len(),
        "No duplicate claims should occur; unique={}, total={}",
        claimed_set.len(),
        claimed.len()
    );

    // Verify: all claimed IDs come from the original set
    for id in claimed_set.iter() {
        assert!(
            expected_ids.contains(id),
            "Claimed ID {id} should be from the original job set"
        );
    }
}
