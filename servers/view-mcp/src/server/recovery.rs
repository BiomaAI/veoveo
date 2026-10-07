//! Hosted lifetime owns deferred recovery independently of capture worker clones.
use super::{AppState, tasks::recover_tasks};
use anyhow::Result;
use futures::StreamExt;
use std::{future::Future, sync::Arc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use veoveo_task_runtime::TaskRecoveryStream;

pub(super) struct RecoveryObserver {
    cancellation: CancellationToken,
    join: JoinHandle<Result<()>>,
}

impl RecoveryObserver {
    pub(super) fn start(state: Arc<AppState>, mut recovery: TaskRecoveryStream) -> Self {
        Self::spawn(move |cancellation| async move {
            loop {
                let report = tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => return Ok(()),
                    report = recovery.next() => match report {
                        Some(report) => report?,
                        None => return Ok(()),
                    },
                };
                // A current owner/snapshot check and durable claim precede scheduling.
                recover_tasks(state.clone(), report.resumable).await?;
            }
        })
    }

    fn spawn<F, Fut>(work: F) -> Self
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let cancellation = CancellationToken::new();
        let join = tokio::spawn(work(cancellation.clone()));
        Self { cancellation, join }
    }

    pub(super) async fn serve(mut self, serving: impl Future<Output = Result<()>>) -> Result<()> {
        tokio::pin!(serving);
        let result = tokio::select! {
            result = &mut serving => {
                self.cancellation.cancel();
                // Recovery performs only bounded Store operations and scheduling,
                // never renderer work. Abort a stalled Store request after five seconds.
                match tokio::time::timeout(std::time::Duration::from_secs(5), &mut self.join).await {
                    Ok(observer) => { observer??; }
                    Err(_) => anyhow::bail!("View recovery observer did not drain within five seconds"),
                }
                result
            }
            observer = &mut self.join => {
                observer??;
                // Exhausting the finite startup set is a healthy completion.
                serving.await
            }
        };
        result
    }
}

impl Drop for RecoveryObserver {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.join.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn hosted_exit_cancels_and_drains_recovery_with_retained_domain_clones() {
        let retained = Arc::new(AtomicBool::new(false));
        let observed = retained.clone();
        let observer = RecoveryObserver::spawn(move |cancel| async move {
            cancel.cancelled().await;
            observed.store(true, Ordering::SeqCst);
            Ok(())
        });
        observer.serve(async { Ok(()) }).await.unwrap();
        assert!(retained.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn exhausted_startup_set_keeps_healthy_server_running() {
        let observer = RecoveryObserver::spawn(|_| async { Ok(()) });
        let (exit, stopped) = tokio::sync::oneshot::channel();
        let serving = observer.serve(async {
            stopped.await?;
            Ok(())
        });
        tokio::pin!(serving);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(200), &mut serving)
                .await
                .is_err()
        );
        exit.send(()).unwrap();
        serving.await.unwrap();
    }

    #[tokio::test]
    async fn stalled_admission_is_aborted_after_five_second_hosted_drain() {
        struct Aborted(Arc<AtomicBool>);
        impl Drop for Aborted {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let aborted = Arc::new(AtomicBool::new(false));
        let observed = aborted.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let observer = RecoveryObserver::spawn(move |_| async move {
            let _guard = Aborted(observed);
            started.send(()).unwrap();
            // A noncooperative admission future models a stalled Store request.
            std::future::pending::<Result<()>>().await
        });
        ready.await.unwrap();
        let start = std::time::Instant::now();
        let error = observer.serve(async { Ok(()) }).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("did not drain within five seconds")
        );
        assert!(start.elapsed() >= std::time::Duration::from_secs(5));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !aborted.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("observer abort did not drop stalled admission");
    }

    #[tokio::test]
    async fn observer_failure_ends_hosted_serving() {
        let observer = RecoveryObserver::spawn(|_| async { anyhow::bail!("recovery refused") });
        let result = observer.serve(std::future::pending()).await;
        assert!(result.unwrap_err().to_string().contains("recovery refused"));
    }
}
