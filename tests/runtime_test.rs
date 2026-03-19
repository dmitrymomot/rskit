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
fn test_task_trait_is_implementable() {
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
