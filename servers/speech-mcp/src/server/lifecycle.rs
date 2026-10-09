//! Keep HTTP shutdown owned after the recovery observer refuses further serving.
use anyhow::Result;
use std::{future::Future, time::Duration};
use tokio_util::sync::CancellationToken;
use veoveo_task_runtime::TaskRecoveryObserver;

pub(super) async fn recover(
    service: std::sync::Arc<crate::application::SpeechService>,
    report: veoveo_task_runtime::RecoveryReport,
) -> Result<()> {
    for snapshot in report.resumable {
        if let Err(error) = service.resume(snapshot.clone()).await {
            reconcile_claim(&service.tasks, &snapshot, error).await?;
        }
    }
    Ok(())
}

async fn reconcile_claim(
    runtime: &veoveo_task_runtime::TaskRuntime,
    admitted: &veoveo_task_runtime::TaskSnapshot,
    error: anyhow::Error,
) -> Result<()> {
    use veoveo_task_runtime::{TaskError, TaskStatus, TaskTransition};
    match error.downcast_ref::<TaskError>() {
        Some(
            TaskError::LeaseHeld(_)
            | TaskError::Conflict(_)
            | TaskError::NotFound(_)
            | TaskError::InvalidTransition { .. },
        ) => {}
        _ => return Err(error),
    }
    let Some(current) = runtime.get_for_recovery(admitted.task_id).await? else {
        return Ok(());
    };
    crate::application::validate_recovery_snapshot(admitted, &current)?;
    if current.is_terminal() {
        return Ok(());
    }
    let live_lease = current
        .lease_expires_at
        .is_some_and(|expiry| expiry > chrono::Utc::now());
    if current.status == TaskStatus::CancelRequested && !live_lease {
        runtime
            .transition_if_current(&current, TaskTransition::Cancelled)
            .await?;
        return Ok(());
    }
    runtime.reconcile_recovery_claim(admitted, error).await
}

pub(super) async fn serve(
    observer: TaskRecoveryObserver,
    serving: impl Future<Output = Result<()>>,
    stop: CancellationToken,
) -> Result<()> {
    serve_with_deadline(observer, serving, stop, Duration::from_secs(30)).await
}

async fn serve_with_deadline(
    observer: TaskRecoveryObserver,
    serving: impl Future<Output = Result<()>>,
    stop: CancellationToken,
    deadline: Duration,
) -> Result<()> {
    tokio::pin!(serving);
    // Keep the timeout future outside observation so error supervision cannot
    // restart the HTTP grace period after shutdown already began.
    let bounded = async {
        tokio::select! {
            result = &mut serving => result,
            _ = stop.cancelled() => tokio::time::timeout(deadline, &mut serving)
                .await.map_err(|_| anyhow::anyhow!("Speech HTTP shutdown deadline exceeded"))
                .and_then(|result| result),
        }
    };
    tokio::pin!(bounded);
    let mut http_finished = false;
    let result = observer
        .serve(async {
            let result = (&mut bounded).await;
            http_finished = true;
            result
        })
        .await;
    stop.cancel();
    let drained = if http_finished {
        Ok(())
    } else {
        (&mut bounded).await
    };
    result?;
    drained
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[tokio::test]
    async fn recovery_failure_still_cancels_and_drains_http() -> Result<()> {
        let stream = futures::stream::iter([
            Ok(veoveo_task_runtime::RecoveryReport::default()),
            Err(veoveo_task_runtime::TaskError::InvalidRecord(
                "recovery refused".into(),
            )),
        ]);
        let observer = TaskRecoveryObserver::start(Box::pin(stream), |_| async { Ok(()) }).await?;
        let stop = CancellationToken::new();
        let drained = Arc::new(AtomicBool::new(false));
        let observed = drained.clone();
        let shutdown = stop.clone();
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            serve(
                observer,
                async move {
                    shutdown.cancelled().await;
                    observed.store(true, Ordering::SeqCst);
                    Ok(())
                },
                stop,
            ),
        )
        .await?
        .unwrap_err();
        anyhow::ensure!(error.to_string().contains("recovery refused"));
        anyhow::ensure!(drained.load(Ordering::SeqCst));
        Ok(())
    }

    #[tokio::test]
    async fn observer_failure_during_shutdown_keeps_the_original_http_deadline() -> Result<()> {
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let stream = futures::stream::once(async { Ok(Default::default()) }).chain(
            futures::stream::once(async move {
                shutdown.cancelled().await;
                tokio::time::sleep(Duration::from_millis(150)).await;
                Err(veoveo_task_runtime::TaskError::InvalidRecord(
                    "deferred refusal".into(),
                ))
            }),
        );
        let observer = TaskRecoveryObserver::start(Box::pin(stream), |_| async { Ok(()) }).await?;
        stop.cancel();
        let started = std::time::Instant::now();
        let error = serve_with_deadline(
            observer,
            std::future::pending(),
            stop,
            Duration::from_millis(250),
        )
        .await
        .unwrap_err();
        anyhow::ensure!(error.to_string().contains("deferred refusal"));
        // A replacement timeout at the observer failure would finish after 400 ms.
        anyhow::ensure!(started.elapsed() < Duration::from_millis(350));
        anyhow::ensure!(started.elapsed() >= Duration::from_millis(250));
        Ok(())
    }

    #[tokio::test]
    async fn http_completion_precedes_a_stalled_observer_drain() -> Result<()> {
        struct Aborted {
            http: Arc<AtomicBool>,
            ordered: Arc<AtomicBool>,
        }
        impl Drop for Aborted {
            fn drop(&mut self) {
                self.ordered
                    .store(self.http.load(Ordering::SeqCst), Ordering::SeqCst);
            }
        }
        let http = Arc::new(AtomicBool::new(false));
        let ordered = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(tokio::sync::Notify::new());
        let started = ready.clone();
        let seen_http = http.clone();
        let seen_ordered = ordered.clone();
        let mut baseline = true;
        let stream = futures::stream::iter([Ok(Default::default()), Ok(Default::default())]);
        let observer = TaskRecoveryObserver::start(Box::pin(stream), move |_| {
            let initial = std::mem::replace(&mut baseline, false);
            let guard = Aborted {
                http: seen_http.clone(),
                ordered: seen_ordered.clone(),
            };
            let started = started.clone();
            async move {
                if initial {
                    return Ok(());
                }
                let _guard = guard;
                started.notify_one();
                std::future::pending().await
            }
        })
        .await?;
        ready.notified().await;
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        stop.cancel();
        let error = serve_with_deadline(
            observer,
            async move {
                shutdown.cancelled().await;
                tokio::time::sleep(Duration::from_millis(20)).await;
                http.store(true, Ordering::SeqCst);
                Ok(())
            },
            stop,
            Duration::from_millis(100),
        )
        .await
        .unwrap_err();
        anyhow::ensure!(
            error
                .to_string()
                .contains("did not drain within five seconds")
        );
        tokio::time::timeout(Duration::from_secs(1), async {
            while !ordered.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        Ok(())
    }
}
