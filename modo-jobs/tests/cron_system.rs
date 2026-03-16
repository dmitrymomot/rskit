//! Dependency smoke tests for cron scheduling infrastructure.
//!
//! These tests validate the upstream `cron` and `tokio_util` crates that
//! modo-jobs relies on for cron scheduling — they don't exercise modo-jobs
//! APIs directly, but guard against regressions in key dependency behaviour.

use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;

#[test]
fn cron_expression_parses_and_schedules() {
    // "0 * * * * *" = every minute (at second 0)
    let schedule = Schedule::from_str("0 * * * * *").expect("Failed to parse cron expression");
    let next = schedule.upcoming(Utc).next();
    assert!(next.is_some(), "Should have a next fire time");

    let next_time = next.unwrap();
    let now = Utc::now();
    let diff = (next_time - now).num_seconds();
    assert!(
        (0..=60).contains(&diff),
        "Next fire time should be within 60s, got {diff}s"
    );
}

#[test]
fn cron_every_second_expression() {
    // "* * * * * *" = every second
    let schedule = Schedule::from_str("* * * * * *").expect("Failed to parse cron expression");
    let now = Utc::now();
    let upcoming: Vec<_> = schedule.upcoming(Utc).take(5).collect();

    assert_eq!(upcoming.len(), 5, "Should have 5 upcoming fire times");

    for (i, time) in upcoming.iter().enumerate() {
        let diff = (*time - now).num_seconds();
        assert!(
            (0..=5).contains(&diff),
            "Fire time {i} should be within 5s of now, got {diff}s"
        );
    }
}

#[tokio::test]
async fn cron_loop_respects_cancellation() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio_util::sync::CancellationToken;

    let cancel = CancellationToken::new();
    let tick_count = Arc::new(AtomicU32::new(0));
    let tick_count_clone = tick_count.clone();
    let cancel_clone = cancel.clone();

    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => {
                    break;
                }
                _ = interval.tick() => {
                    tick_count_clone.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    });

    // Let it tick a few times
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count_before_cancel = tick_count.load(Ordering::SeqCst);
    assert!(
        count_before_cancel >= 2,
        "Should have ticked at least twice before cancel, got {count_before_cancel}"
    );

    // Cancel and wait for the task to fully exit
    cancel.cancel();
    handle.await.expect("Task panicked");

    // Record count after the task has exited
    let count_at_exit = tick_count.load(Ordering::SeqCst);

    // After a further delay, verify no more ticks occur
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count_after_wait = tick_count.load(Ordering::SeqCst);
    assert_eq!(
        count_at_exit, count_after_wait,
        "No more ticks should occur after cancellation (at_exit={count_at_exit}, after_wait={count_after_wait})"
    );
}

#[test]
fn cron_schedule_exhaustion() {
    // "0 0 * * * *" = every hour (at minute 0, second 0)
    let schedule = Schedule::from_str("0 0 * * * *").expect("Failed to parse cron expression");

    // An hourly schedule should always have upcoming times
    let upcoming: Vec<_> = schedule.upcoming(Utc).take(10).collect();
    assert_eq!(
        upcoming.len(),
        10,
        "Hourly schedule should always have upcoming fire times"
    );

    // Verify each subsequent time is about 1 hour apart
    for window in upcoming.windows(2) {
        let diff = (window[1] - window[0]).num_seconds();
        assert_eq!(diff, 3600, "Each pair should be 1 hour apart, got {diff}s");
    }
}
