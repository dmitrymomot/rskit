mod common;

use common::setup_db;
use modo_db::sea_orm::sea_query::Expr;
use modo_db::sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, ExprTrait, QueryFilter,
};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::{JobId, JobState};

async fn insert_running_job(
    db: &modo_db::sea_orm::DatabaseConnection,
    id: &JobId,
    locked_at: chrono::DateTime<chrono::Utc>,
    attempts: i32,
) {
    let now = chrono::Utc::now();
    let model = jobs_entity::ActiveModel {
        id: ActiveValue::Set(id.as_str().to_string()),
        name: ActiveValue::Set("test_job".to_string()),
        queue: ActiveValue::Set("default".to_string()),
        payload: ActiveValue::Set("{}".to_string()),
        state: ActiveValue::Set(JobState::Running.as_str().to_string()),
        priority: ActiveValue::Set(0),
        attempts: ActiveValue::Set(attempts),
        max_attempts: ActiveValue::Set(5),
        run_at: ActiveValue::Set(now),
        timeout_secs: ActiveValue::Set(300),
        locked_by: ActiveValue::Set(Some("worker-1".to_string())),
        locked_at: ActiveValue::Set(Some(locked_at)),
        last_error: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    model.insert(db).await.expect("Insert failed");
}

#[tokio::test]
async fn reaper_requeues_stale_running_job() {
    let db = setup_db().await;
    let now = chrono::Utc::now();

    // Stale job: locked 20 minutes ago (threshold is 10 min = 600s)
    let stale_id = JobId::new();
    let stale_locked_at = now - chrono::Duration::minutes(20);
    insert_running_job(&db, &stale_id, stale_locked_at, 3).await;

    // Fresh job: locked 2 minutes ago (well within threshold)
    let fresh_id = JobId::new();
    let fresh_locked_at = now - chrono::Duration::minutes(2);
    insert_running_job(&db, &fresh_id, fresh_locked_at, 2).await;

    // Simulate reaper: threshold = 10 minutes (600 seconds)
    let threshold_secs: i64 = 600;
    let cutoff = now - chrono::Duration::seconds(threshold_secs);

    let update = jobs_entity::Entity::update_many()
        .filter(jobs_entity::Column::State.eq(JobState::Running.as_str()))
        .filter(jobs_entity::Column::LockedAt.lt(cutoff))
        .col_expr(
            jobs_entity::Column::State,
            Expr::value(JobState::Pending.as_str()),
        )
        .col_expr(
            jobs_entity::Column::Attempts,
            Expr::col(jobs_entity::Column::Attempts).sub(1),
        )
        .col_expr(
            jobs_entity::Column::LockedBy,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            jobs_entity::Column::LockedAt,
            Expr::value(Option::<chrono::DateTime<chrono::Utc>>::None),
        )
        .col_expr(jobs_entity::Column::UpdatedAt, Expr::value(now));

    let result = update.exec(&db).await.expect("Reaper update failed");
    assert_eq!(
        result.rows_affected, 1,
        "Should have reaped exactly 1 stale job"
    );

    // Verify stale job is now pending with decremented attempts and cleared lock
    let stale_job = jobs_entity::Entity::find_by_id(stale_id.as_str())
        .one(&db)
        .await
        .expect("Query failed")
        .expect("Stale job not found");

    assert_eq!(stale_job.state, "pending", "Stale job should be pending");
    assert_eq!(
        stale_job.attempts, 2,
        "Stale job attempts should be decremented from 3 to 2"
    );
    assert!(
        stale_job.locked_by.is_none(),
        "Stale job locked_by should be cleared"
    );
    assert!(
        stale_job.locked_at.is_none(),
        "Stale job locked_at should be cleared"
    );

    // Verify fresh job is unchanged
    let fresh_job = jobs_entity::Entity::find_by_id(fresh_id.as_str())
        .one(&db)
        .await
        .expect("Query failed")
        .expect("Fresh job not found");

    assert_eq!(
        fresh_job.state, "running",
        "Fresh job should still be running"
    );
    assert_eq!(
        fresh_job.attempts, 2,
        "Fresh job attempts should be unchanged"
    );
    assert_eq!(
        fresh_job.locked_by.as_deref(),
        Some("worker-1"),
        "Fresh job locked_by should be unchanged"
    );
    assert!(
        fresh_job.locked_at.is_some(),
        "Fresh job locked_at should still be set"
    );
}
