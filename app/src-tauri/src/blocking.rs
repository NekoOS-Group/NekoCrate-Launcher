//! Share Tokio's blocking-task machinery for operations returned directly to IPC.
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use tauri::async_runtime::JoinHandle;

pub fn json<T: Serialize>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> JoinHandle<Result<Value>> {
    value(move || Ok(serde_json::json!(operation()?)))
}

// Already-built JSON can enter the same task without serializing it again.
pub fn value(
    operation: impl FnOnce() -> Result<Value> + Send + 'static,
) -> JoinHandle<Result<Value>> {
    run(operation)
}

// Intermediate results stay typed. Sharing the task for each result type also
// avoids separate task implementations for several closures returning PathBuf.
pub fn run<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> JoinHandle<Result<T>> {
    spawn(Box::new(operation))
}

// Erase the closure before entering Tokio. Direct IPC results also share Value
// as their output, so they use one task, join and drop state machine.
// The boundary must survive LTO; only UI operations pay this one allocation.
#[inline(never)]
fn spawn<T: Send + 'static>(
    operation: Box<dyn FnOnce() -> Result<T> + Send>,
) -> JoinHandle<Result<T>> {
    tauri::async_runtime::spawn_blocking(operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn results_keep_their_json_shape_and_run_off_the_caller_thread() {
        #[derive(Serialize)]
        struct Item {
            name: String,
            optional: Option<u64>,
            size: u64,
        }
        let caller = std::thread::current().id();
        let result = json(move || {
            assert_ne!(std::thread::current().id(), caller);
            Ok(Item {
                name: "猫咪".into(),
                optional: None,
                size: u64::MAX,
            })
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            result,
            serde_json::json!({"name":"猫咪","optional":null,"size":u64::MAX})
        );
        assert_eq!(
            json(|| Ok(vec![1, 2])).await.unwrap().unwrap(),
            serde_json::json!([1, 2])
        );
    }

    #[tokio::test]
    async fn operation_errors_keep_their_source_chain() {
        let error = json::<()>(|| Err(anyhow::anyhow!("cause").context("operation")))
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(format!("{error:#}"), "operation: cause");
    }

    #[tokio::test]
    async fn intermediate_results_stay_typed_without_json_conversion() {
        // ThreadId has no Serialize implementation; typed operations must not
        // acquire JSON constraints or execute on the caller's runtime thread.
        let caller = std::thread::current().id();
        let worker = run(|| Ok(std::thread::current().id()))
            .await
            .unwrap()
            .unwrap();
        assert_ne!(worker, caller);
        let error = run::<()>(|| Err(anyhow::anyhow!("cause").context("typed operation")))
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(format!("{error:#}"), "typed operation: cause");
    }

    #[tokio::test]
    async fn panics_are_still_join_errors() {
        assert!(json::<()>(|| panic!("fixture worker panic")).await.is_err());
    }
}
