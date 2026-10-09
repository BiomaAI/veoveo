//! Hosted lifetime owns finite startup recovery independently of domain worker clones.
use super::{RecoveryReport, TaskRecoveryStream};
use anyhow::Result;
use futures::StreamExt;
use std::future::Future;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Own a finite startup recovery stream for a hosted service lifetime.
pub struct TaskRecoveryObserver {
    cancellation: CancellationToken,
    join: JoinHandle<Result<()>>,
}

impl TaskRecoveryObserver {
    /// Apply the startup baseline before the owner begins serving, then observe
    /// only retained startup identities. Owner callbacks keep claim authority.
    pub async fn start<F, Fut>(mut recovery: TaskRecoveryStream, mut on_report: F) -> Result<Self>
    where
        F: FnMut(RecoveryReport) -> Fut + Send + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        if let Some(report) = recovery.next().await {
            on_report(report?).await?;
        }
        Ok(Self::start_deferred(recovery, on_report))
    }

    /// Supervise all reports asynchronously when owner admission may wait for capacity.
    /// The caller must establish the recovery stream baseline before serving.
    pub fn start_deferred<F, Fut>(mut recovery: TaskRecoveryStream, mut on_report: F) -> Self
    where
        F: FnMut(RecoveryReport) -> Fut + Send + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
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
                on_report(report).await?;
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

    /// Keep serving after finite recovery completes; fail on observation or callback
    /// errors. Serving exit cancels recovery and permits five seconds for its drain.
    /// Owners with further shutdown duties can borrow a pinned serving future.
    pub async fn serve(mut self, serving: impl Future<Output = Result<()>>) -> Result<()> {
        tokio::pin!(serving);
        let result = tokio::select! {
            result = &mut serving => {
                self.cancellation.cancel();
                // Owners use callbacks for admission and scheduling. Abort a stalled
                // callback or Store request after five seconds.
                match tokio::time::timeout(std::time::Duration::from_secs(5), &mut self.join).await {
                    Ok(observer) => { observer??; }
                    Err(_) => anyhow::bail!("Task recovery observer did not drain within five seconds"),
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

impl Drop for TaskRecoveryObserver {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.join.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[tokio::test]
    async fn hosted_exit_cancels_and_drains_recovery_with_retained_domain_clones() {
        let retained = Arc::new(AtomicBool::new(false));
        let observed = retained.clone();
        let observer = TaskRecoveryObserver::spawn(move |cancel| async move {
            cancel.cancelled().await;
            observed.store(true, Ordering::SeqCst);
            Ok(())
        });
        observer.serve(async { Ok(()) }).await.unwrap();
        assert!(retained.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn exhausted_startup_set_keeps_healthy_server_running() {
        let observer =
            TaskRecoveryObserver::start(Box::pin(futures::stream::empty()), |_| async { Ok(()) })
                .await
                .unwrap();
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
        let ready = Arc::new(tokio::sync::Notify::new());
        let started = ready.clone();
        let mut initial = true;
        let stream =
            futures::stream::iter([Ok(RecoveryReport::default()), Ok(RecoveryReport::default())]);
        let observer = TaskRecoveryObserver::start(Box::pin(stream), move |_| {
            let baseline = std::mem::replace(&mut initial, false);
            let started = started.clone();
            let observed = observed.clone();
            async move {
                // The baseline finishes; the deferred callback models stalled admission.
                if baseline {
                    return Ok(());
                }
                let _guard = Aborted(observed);
                started.notify_one();
                std::future::pending::<Result<()>>().await
            }
        })
        .await
        .unwrap();
        ready.notified().await;
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
        let mut baseline = true;
        let stream =
            futures::stream::iter([Ok(RecoveryReport::default()), Ok(RecoveryReport::default())]);
        let observer = TaskRecoveryObserver::start(Box::pin(stream), move |_| {
            let initial = std::mem::replace(&mut baseline, false);
            async move {
                if initial {
                    Ok(())
                } else {
                    anyhow::bail!("recovery refused")
                }
            }
        })
        .await
        .unwrap();
        let result = observer.serve(std::future::pending()).await;
        assert!(result.unwrap_err().to_string().contains("recovery refused"));
    }
}

#[cfg(test)]
mod start_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn report() -> RecoveryReport {
        RecoveryReport::default()
    }

    #[tokio::test]
    async fn baseline_callback_finishes_before_start_returns_and_deferred_reports_follow() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let stream =
            futures::stream::once(async { Ok(report()) }).chain(futures::stream::once(async {
                receive.await.unwrap();
                Ok(report())
            }));
        let observer = TaskRecoveryObserver::start(Box::pin(stream), move |_| {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        send.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while calls.load(Ordering::SeqCst) != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        observer.serve(async { Ok(()) }).await.unwrap();
    }

    #[tokio::test]
    async fn initial_stream_or_owner_error_refuses_startup() {
        let stream = Box::pin(futures::stream::once(async {
            Err(crate::TaskError::InvalidRecord("bad baseline".into()))
        }));
        assert!(
            TaskRecoveryObserver::start(stream, |_| async { Ok(()) })
                .await
                .is_err()
        );
        let stream = Box::pin(futures::stream::once(async { Ok(report()) }));
        assert!(
            TaskRecoveryObserver::start(stream, |_| async { anyhow::bail!("owner refused") })
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn deferred_stream_error_stops_serving() {
        let stream = futures::stream::iter(vec![
            Ok(report()),
            Err(crate::TaskError::InvalidRecord("deferred refusal".into())),
        ]);
        let observer = TaskRecoveryObserver::start(Box::pin(stream), |_| async { Ok(()) })
            .await
            .unwrap();
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            observer.serve(std::future::pending()),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("deferred refusal"));
    }
}
