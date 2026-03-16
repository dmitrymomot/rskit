mod common;

use common::setup_db;
use modo_db::sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::{JobId, JobState};

async fn insert_job_with_state(
    db: &modo_db::sea_orm::DatabaseConnection,
    id: &JobId,
    state: JobState,
    updated_at: chrono::DateTime<chrono::Utc>,
) {
    let now = chrono::Utc::now();
    let model = jobs_entity::ActiveModel {
        id: ActiveValue::Set(id.as_str().to_string()),
        name: ActiveValue::Set("test_job".to_string()),
        queue: ActiveValue::Set("default".to_string()),
        payload: ActiveValue::Set("{}".to_string()),
        state: ActiveValue::Set(state.as_str().to_string()),
        priority: ActiveValue::Set(0),
        attempts: ActiveValue::Set(1),
        max_attempts: ActiveValue::Set(3),
        run_at: ActiveValue::Set(now),
        timeout_secs: ActiveValue::Set(300),
        locked_by: ActiveValue::Set(None),
        locked_at: ActiveValue::Set(None),
        last_error: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(updated_at),
    };
    model.insert(db).await.expect("Insert failed");
}

#[tokio::test]
async fn cleanup_deletes_old_terminal_jobs() {
    let db = setup_db().await;
    let now = chrono::Utc::now();
    let two_days_ago = now - chrono::Duration::days(2);
    let one_hour_ago = now - chrono::Duration::hours(1);

    // Old terminal jobs (should be deleted)
    let old_completed_id = JobId::new();
    insert_job_with_state(&db, &old_completed_id, JobState::Completed, two_days_ago).await;

    let old_dead_id = JobId::new();
    insert_job_with_state(&db, &old_dead_id, JobState::Dead, two_days_ago).await;

    let old_cancelled_id = JobId::new();
    insert_job_with_state(&db, &old_cancelled_id, JobState::Cancelled, two_days_ago).await;

    // Recent completed job (should survive - within retention)
    let recent_completed_id = JobId::new();
    insert_job_with_state(&db, &recent_completed_id, JobState::Completed, one_hour_ago).await;

    // Old pending job (should survive - not a terminal state)
    let old_pending_id = JobId::new();
    insert_job_with_state(&db, &old_pending_id, JobState::Pending, two_days_ago).await;

    // Simulate cleanup: retention = 86400 seconds (1 day)
    let retention_secs: i64 = 86400;
    let cutoff = now - chrono::Duration::seconds(retention_secs);

    let terminal_states: Vec<String> = vec![
        JobState::Completed.as_str().to_string(),
        JobState::Dead.as_str().to_string(),
        JobState::Cancelled.as_str().to_string(),
    ];

    let result = jobs_entity::Entity::delete_many()
        .filter(jobs_entity::Column::State.is_in(&terminal_states))
        .filter(jobs_entity::Column::UpdatedAt.lt(cutoff))
        .exec(&db)
        .await
        .expect("Cleanup delete failed");

    assert_eq!(
        result.rows_affected, 3,
        "Should have deleted 3 old terminal jobs"
    );

    // Verify old terminal jobs are gone
    let old_completed = jobs_entity::Entity::find_by_id(old_completed_id.as_str())
        .one(&db)
        .await
        .expect("Query failed");
    assert!(
        old_completed.is_none(),
        "Old completed job should be deleted"
    );

    let old_dead = jobs_entity::Entity::find_by_id(old_dead_id.as_str())
        .one(&db)
        .await
        .expect("Query failed");
    assert!(old_dead.is_none(), "Old dead job should be deleted");

    let old_cancelled = jobs_entity::Entity::find_by_id(old_cancelled_id.as_str())
        .one(&db)
        .await
        .expect("Query failed");
    assert!(
        old_cancelled.is_none(),
        "Old cancelled job should be deleted"
    );

    // Verify recent completed job survives
    let recent_completed = jobs_entity::Entity::find_by_id(recent_completed_id.as_str())
        .one(&db)
        .await
        .expect("Query failed");
    assert!(
        recent_completed.is_some(),
        "Recent completed job should survive"
    );

    // Verify old pending job survives
    let old_pending = jobs_entity::Entity::find_by_id(old_pending_id.as_str())
        .one(&db)
        .await
        .expect("Query failed");
    assert!(old_pending.is_some(), "Old pending job should survive");
}
