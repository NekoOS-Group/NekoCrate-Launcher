use anyhow::Result;
use std::future::Future;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Keep the same install future and its open transfers while paused. Cancellation
/// still polls the operation so it can flush its final progress and unwind.
pub async fn run<T>(
    operation: impl Future<Output = Result<T>>,
    cancel: &CancellationToken,
    mut paused: watch::Receiver<bool>,
) -> Result<T> {
    tokio::pin!(operation);
    loop {
        let is_paused = *paused.borrow_and_update();
        if is_paused {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return operation.await,
                changed = paused.changed() => if changed.is_err() { return operation.await; },
            }
        } else {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return operation.await,
                changed = paused.changed() => if changed.is_err() { return operation.await; },
                result = &mut operation => return result,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    #[tokio::test]
    async fn pause_stops_polling_and_resume_keeps_the_same_operation() {
        let (pause, receiver) = watch::channel(false);
        let cancel = CancellationToken::new();
        let starts = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(tokio::sync::Notify::new());
        let next_chunk = Arc::new(tokio::sync::Notify::new());
        let count = starts.clone();
        let started = entered.clone();
        let gate = next_chunk.clone();
        let mut task = tokio::spawn(async move {
            run(
                async move {
                    count.fetch_add(1, Ordering::Relaxed);
                    started.notify_one();
                    gate.notified().await;
                    Ok(42)
                },
                &cancel,
                receiver,
            )
            .await
        });
        entered.notified().await;
        pause.send(true).unwrap();
        next_chunk.notify_one();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut task)
                .await
                .is_err()
        );
        pause.send(false).unwrap();
        assert_eq!(task.await.unwrap().unwrap(), 42);
        assert_eq!(starts.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn cancellation_while_paused_runs_operation_cleanup() {
        let (_pause, receiver) = watch::channel(true);
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let cleaned = Arc::new(AtomicUsize::new(0));
        let captured = cleaned.clone();
        let task = tokio::spawn(async move {
            run(
                async {
                    token.cancelled().await;
                    captured.fetch_add(1, Ordering::Relaxed);
                    anyhow::bail!("操作已取消")
                },
                &token,
                receiver,
            )
            .await as Result<()>
        });
        cancel.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert_eq!(cleaned.load(Ordering::Relaxed), 1);
    }
}
