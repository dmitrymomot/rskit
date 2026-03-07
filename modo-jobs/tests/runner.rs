use modo_db::sea_orm::{
    ActiveModelTrait, ActiveValue, ConnectionTrait, Database, EntityTrait, Schema,
};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::runner;
use modo_jobs::{JobId, JobState};

async fn setup_db() -> modo_db::sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to connect");

    let schema = Schema::new(db.get_database_backend());
    let mut builder = schema.builder();
    let reg = inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_jobs")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder.sync(&db).await.expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.execute_unprepared(sql).await.expect("Extra SQL failed");
    }
    db
}

async fn insert_job(
    db: &modo_db::sea_orm::DatabaseConnection,
    id: &JobId,
    queue: &str,
    priority: i32,
    run_at: chrono::DateTime<chrono::Utc>,
    max_attempts: i32,
) {
    let now = chrono::Utc::now();
    let model = jobs_entity::ActiveModel {
        id: ActiveValue::Set(id.as_str().to_string()),
        name: ActiveValue::Set("test_job".to_string()),
        queue: ActiveValue::Set(queue.to_string()),
        payload: ActiveValue::Set("{}".to_string()),
        state: ActiveValue::Set(JobState::Pending.as_str().to_string()),
        priority: ActiveValue::Set(priority),
        attempts: ActiveValue::Set(0),
        max_attempts: ActiveValue::Set(max_attempts),
        run_at: ActiveValue::Set(run_at),
        timeout_secs: ActiveValue::Set(300),
        locked_by: ActiveValue::Set(None),
        locked_at: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    model.insert(db).await.expect("Insert failed");
}

#[tokio::test]
async fn test_claim_next_returns_pending_job() {
    let db = setup_db().await;
    let id = JobId::new();
    let now = chrono::Utc::now();
    insert_job(&db, &id, "default", 0, now, 3).await;

    let claimed = runner::claim_next(&db, "default", "worker-1")
        .await
        .expect("Claim failed");

    let job = claimed.expect("Expected a job");
    assert_eq!(job.id, id.as_str());
    assert_eq!(job.state, "running");
    assert_eq!(job.attempts, 1);
    assert_eq!(job.locked_by.as_deref(), Some("worker-1"));
}

#[tokio::test]
async fn test_claim_next_respects_run_at() {
    let db = setup_db().await;
    let id = JobId::new();
    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    insert_job(&db, &id, "default", 0, future, 3).await;

    let claimed = runner::claim_next(&db, "default", "worker-1")
        .await
        .expect("Claim failed");

    assert!(claimed.is_none(), "Should not claim future job");
}

#[tokio::test]
async fn test_claim_next_priority_ordering() {
    let db = setup_db().await;
    let now = chrono::Utc::now();

    let low_id = JobId::new();
    let high_id = JobId::new();
    insert_job(&db, &low_id, "default", 0, now, 3).await;
    insert_job(&db, &high_id, "default", 10, now, 3).await;

    let claimed = runner::claim_next(&db, "default", "worker-1")
        .await
        .expect("Claim failed")
        .expect("Expected a job");

    assert_eq!(
        claimed.id,
        high_id.as_str(),
        "Higher priority should be claimed first"
    );
}

#[tokio::test]
async fn test_mark_completed() {
    let db = setup_db().await;
    let id = JobId::new();
    let now = chrono::Utc::now();
    insert_job(&db, &id, "default", 0, now, 3).await;

    // Claim it first
    runner::claim_next(&db, "default", "worker-1")
        .await
        .expect("Claim failed");

    // Mark completed
    runner::mark_completed(&db, id.as_str()).await;

    let job = jobs_entity::Entity::find_by_id(id.as_str())
        .one(&db)
        .await
        .expect("Query failed")
        .expect("Job not found");

    assert_eq!(job.state, "completed");
}

#[tokio::test]
async fn test_mark_failed_sets_pending_with_future_run_at() {
    let db = setup_db().await;
    let id = JobId::new();
    let now = chrono::Utc::now();
    insert_job(&db, &id, "default", 0, now, 3).await;

    // Claim it
    let claimed = runner::claim_next(&db, "default", "worker-1")
        .await
        .expect("Claim failed")
        .expect("Expected a job");

    // Mark failed (retry)
    runner::mark_failed(&db, &claimed).await;

    let job = jobs_entity::Entity::find_by_id(id.as_str())
        .one(&db)
        .await
        .expect("Query failed")
        .expect("Job not found");

    assert_eq!(job.state, "pending");
    assert!(
        job.run_at > now,
        "run_at should be in the future for backoff"
    );
    assert!(job.locked_by.is_none());
    assert!(job.locked_at.is_none());
}

#[tokio::test]
async fn test_handle_failure_marks_dead_after_max_attempts() {
    let db = setup_db().await;
    let id = JobId::new();
    let now = chrono::Utc::now();
    // max_attempts = 1 means only 1 attempt allowed
    insert_job(&db, &id, "default", 0, now, 1).await;

    // Claim it (sets attempts = 1)
    let claimed = runner::claim_next(&db, "default", "worker-1")
        .await
        .expect("Claim failed")
        .expect("Expected a job");

    assert_eq!(claimed.attempts, 1);
    assert_eq!(claimed.max_attempts, 1);

    // handle_failure should mark dead since attempts >= max_attempts
    runner::handle_failure(&db, &claimed).await;

    let job = jobs_entity::Entity::find_by_id(id.as_str())
        .one(&db)
        .await
        .expect("Query failed")
        .expect("Job not found");

    assert_eq!(job.state, "dead");
}
