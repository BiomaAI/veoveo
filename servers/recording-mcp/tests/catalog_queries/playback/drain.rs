//! Retained serving-task ownership across interrupted fixture transport drains.
use anyhow::{Context, Result, ensure};
use std::time::{Duration, Instant};

pub(crate) struct ServingTask {
    task: Option<tokio::task::JoinHandle<Result<()>>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    end: Option<Instant>,
    abort_end: Option<Instant>,
    closed: bool,
    failed: bool,
}
impl ServingTask {
    pub fn new(
        task: tokio::task::JoinHandle<Result<()>>,
        shutdown: tokio::sync::oneshot::Sender<()>,
    ) -> Self {
        Self {
            task: Some(task),
            shutdown: Some(shutdown),
            end: None,
            abort_end: None,
            closed: false,
            failed: false,
        }
    }
    pub async fn close(&mut self) -> Result<()> {
        if self.closed {
            ensure!(
                !self.failed,
                "fixture serving task previously failed to drain"
            );
            return Ok(());
        }
        let end = *self
            .end
            .get_or_insert_with(|| Instant::now() + Duration::from_secs(5));
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        // Borrow the retained JoinHandle. Cancellation cannot detach or replace it.
        if self.abort_end.is_none() {
            let task = self
                .task
                .as_mut()
                .context("fixture serving task ownership missing")?;
            let joined = if Instant::now() >= end {
                None
            } else {
                tokio::time::timeout_at(end.into(), task).await.ok()
            };
            match joined {
                Some(result) => {
                    self.failed |= !matches!(result, Ok(Ok(())));
                    self.task = None;
                    self.closed = true;
                }
                None => {
                    self.failed = true;
                    self.task.as_ref().unwrap().abort();
                    self.abort_end = Some(Instant::now() + Duration::from_secs(1));
                }
            }
        }
        if !self.closed {
            let end = self.abort_end.context("fixture abort deadline missing")?;
            let task = self
                .task
                .as_mut()
                .context("fixture aborted task ownership missing")?;
            if tokio::time::timeout_at(end.into(), task).await.is_ok() {
                self.task = None;
                self.closed = true;
            }
        }
        ensure!(
            self.closed && !self.failed,
            "fixture serving task drain failed or exceeded its original deadline"
        );
        Ok(())
    }
}
impl Drop for ServingTask {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.as_ref() {
            task.abort();
        }
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
    async fn interrupted_close_retains_join_and_deadline_and_failure_is_sticky() -> Result<()> {
        let (shutdown, stopped) = tokio::sync::oneshot::channel();
        let release = Arc::new(tokio::sync::Notify::new());
        let running = release.clone();
        let completed = Arc::new(AtomicBool::new(false));
        let done = completed.clone();
        let task = tokio::spawn(async move {
            let _ = stopped.await;
            running.notified().await;
            done.store(true, Ordering::SeqCst);
            Ok(())
        });
        let mut serving = ServingTask::new(task, shutdown);
        ensure!(
            tokio::time::timeout(Duration::from_millis(1), serving.close())
                .await
                .is_err()
        );
        let end = serving.end;
        ensure!(serving.task.is_some() && !serving.closed && serving.shutdown.is_none());
        release.notify_one();
        serving.close().await?;
        ensure!(
            serving.closed
                && serving.task.is_none()
                && serving.end == end
                && completed.load(Ordering::SeqCst)
        );

        let (shutdown, stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = stopped.await;
            std::future::pending::<Result<()>>().await
        });
        let mut failed = ServingTask::new(task, shutdown);
        failed.end = Some(Instant::now());
        ensure!(
            failed.close().await.is_err()
                && failed.failed
                && failed.closed
                && failed.task.is_none()
        );
        let end = failed.end;
        let abort_end = failed.abort_end;
        ensure!(
            failed.close().await.is_err() && failed.end == end && failed.abort_end == abort_end
        );
        Ok(())
    }
}
