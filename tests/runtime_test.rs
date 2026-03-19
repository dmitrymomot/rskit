use modo::error::Result;
use modo::runtime::Task;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct MockTask {
    shutdown_called: Arc<AtomicBool>,
}

impl Task for MockTask {
    async fn shutdown(self) -> Result<()> {
        self.shutdown_called.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn test_mock_task_constructs() {
    let flag = Arc::new(AtomicBool::new(false));
    let _task = MockTask {
        shutdown_called: flag,
    };
}

#[tokio::test]
async fn test_task_shutdown() {
    let flag = Arc::new(AtomicBool::new(false));
    let task = MockTask {
        shutdown_called: flag.clone(),
    };

    assert!(!flag.load(Ordering::SeqCst));
    task.shutdown().await.unwrap();
    assert!(flag.load(Ordering::SeqCst));
}

#[test]
fn test_run_macro_compiles_single_task() {
    let flag = Arc::new(AtomicBool::new(false));
    let task = MockTask {
        shutdown_called: flag,
    };
    let _future = modo::run!(task);
}

#[test]
fn test_run_macro_compiles_multiple_tasks() {
    let f1 = Arc::new(AtomicBool::new(false));
    let f2 = Arc::new(AtomicBool::new(false));
    let t1 = MockTask {
        shutdown_called: f1,
    };
    let t2 = MockTask {
        shutdown_called: f2,
    };
    let _future = modo::run!(t1, t2);
}

#[test]
fn test_run_macro_compiles_trailing_comma() {
    let flag = Arc::new(AtomicBool::new(false));
    let task = MockTask {
        shutdown_called: flag,
    };
    let _future = modo::run!(task,);
}
