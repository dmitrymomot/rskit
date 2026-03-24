use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use modo::Service;
use modo::cron::{CronOptions, Scheduler};
use modo::error::Result;
use modo::service::Registry;

async fn counting_job(Service(counter): Service<Arc<AtomicU32>>) -> Result<()> {
    counter.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

async fn slow_job(Service(counter): Service<Arc<AtomicU32>>) -> Result<()> {
    tokio::time::sleep(Duration::from_secs(10)).await;
    counter.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

#[tokio::test]
async fn scheduler_runs_job_on_interval() {
    let counter = Arc::new(AtomicU32::new(0));
    let mut registry = Registry::new();
    registry.add(counter.clone());

    let scheduler = Scheduler::builder(&registry)
        .job("@every 1s", counting_job)
        .start()
        .await;

    tokio::time::sleep(Duration::from_millis(2500)).await;

    modo::runtime::Task::shutdown(scheduler).await.unwrap();

    let count = counter.load(Ordering::SeqCst);
    assert!(count >= 2, "expected at least 2 executions, got {count}");
}

#[tokio::test]
async fn scheduler_shutdown_is_clean() {
    let mut registry = Registry::new();
    registry.add(Arc::new(AtomicU32::new(0)));

    let scheduler = Scheduler::builder(&registry)
        .job("@every 1s", counting_job)
        .start()
        .await;

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        modo::runtime::Task::shutdown(scheduler),
    )
    .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn scheduler_skips_overlapping_runs() {
    let counter = Arc::new(AtomicU32::new(0));
    let mut registry = Registry::new();
    registry.add(counter.clone());

    let scheduler = Scheduler::builder(&registry)
        .job_with("@every 1s", slow_job, {
            let mut o = CronOptions::default();
            o.timeout_secs = 30;
            o
        })
        .start()
        .await;

    tokio::time::sleep(Duration::from_millis(3500)).await;

    modo::runtime::Task::shutdown(scheduler).await.unwrap();

    let count = counter.load(Ordering::SeqCst);
    assert!(count <= 1, "expected at most 1 execution, got {count}");
}

#[tokio::test]
async fn scheduler_timeout_cancels_job() {
    let counter = Arc::new(AtomicU32::new(0));
    let mut registry = Registry::new();
    registry.add(counter.clone());

    let scheduler = Scheduler::builder(&registry)
        .job_with("@every 1s", slow_job, {
            let mut o = CronOptions::default();
            o.timeout_secs = 1;
            o
        })
        .start()
        .await;

    tokio::time::sleep(Duration::from_millis(2500)).await;

    modo::runtime::Task::shutdown(scheduler).await.unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 0);
}
