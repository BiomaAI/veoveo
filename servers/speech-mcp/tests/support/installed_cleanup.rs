//! Retain actual SDK handles and their original consuming close futures.
use anyhow::{Context, Result, ensure};
use rmcp::service::Subscription;
use std::{
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};
use veoveo_testing_support::{SmokeMcpClient, lifecycle::owner};

type Closing = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

pub(super) struct Slot<H> {
    pub handle: Option<H>,
    closing: Option<Closing>,
    end: Option<Instant>,
    pub closed: bool,
    pub failed: bool,
}
impl<H> Default for Slot<H> {
    fn default() -> Self {
        Self {
            handle: None,
            closing: None,
            end: None,
            closed: true,
            failed: false,
        }
    }
}
impl<H> Slot<H> {
    pub fn retain(&mut self, handle: H) {
        self.handle = Some(handle);
        self.closed = false;
    }
    pub async fn close<F: Future<Output = Result<()>> + Send + 'static>(
        &mut self,
        close: impl FnOnce(H) -> F,
        deadline: Instant,
    ) -> Result<()> {
        ensure!(
            !self.failed,
            "SDK close previously failed; outcome is sticky"
        );
        if self.closed {
            return Ok(());
        }
        // Re-poll the original future if its caller was dropped; never repeat cancel.
        let end = *self
            .end
            .get_or_insert_with(|| (Instant::now() + Duration::from_secs(10)).min(deadline));
        if self.closing.is_none() {
            let handle = self
                .handle
                .take()
                .context("SDK handle close ownership missing")?;
            self.closing = Some(Box::pin(close(handle)));
        }
        let result = tokio::time::timeout_at(end.into(), self.closing.as_mut().unwrap()).await;
        match result {
            Ok(Ok(())) => {
                self.closing = None;
                self.closed = true;
            }
            Ok(Err(_)) | Err(_) => {
                self.failed = true;
            }
        }
        ensure!(
            self.closed && !self.failed,
            "SDK close failed or exceeded its original deadline"
        );
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Handles {
    pub caller: Slot<SmokeMcpClient>,
    pub listener: Slot<Subscription>,
    pub resource_listener: Slot<Subscription>,
    pub recovery_listener: Slot<Subscription>,
    pub crash_watch: Slot<veoveo_testing_support::installed::restart::CrashWatch>,
    pub crash_receipt: Option<veoveo_testing_support::installed::restart::CrashReceipt>,
}
impl Handles {
    pub async fn close(&mut self) -> Result<()> {
        let deadline = owner::cleanup_deadline()?;
        if let Some(watch) = &self.crash_watch.handle {
            self.crash_receipt = watch.snapshot();
        }
        let watch = self
            .crash_watch
            .close(
                |watch| async move {
                    ensure!(watch.close().await, "Speech crash watch close failed");
                    Ok(())
                },
                deadline,
            )
            .await;
        let recovery = self
            .recovery_listener
            .close(
                |mut listener| async move { listener.cancel().await.map_err(anyhow::Error::from) },
                deadline,
            )
            .await;
        let resource = self
            .resource_listener
            .close(
                |mut listener| async move { listener.cancel().await.map_err(anyhow::Error::from) },
                deadline,
            )
            .await;
        let listener = self
            .listener
            .close(
                |mut listener| async move { listener.cancel().await.map_err(anyhow::Error::from) },
                deadline,
            )
            .await;
        // Preserve the caller's opportunity inside the same grace on listener failure.
        let caller = self.caller.close(|caller| caller.cancel(), deadline).await;
        watch.and(recovery).and(resource).and(listener).and(caller)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn original_close_future_survives_drop_and_owner_cleanup() -> Result<()> {
        const CHILD: &str = "VEOVEO_SPEECH_CLOSE_CONTROL_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let directory = tempfile::tempdir()?;
            let mut command = std::process::Command::new(std::env::current_exe()?);
            veoveo_testing_support::configure_binary_runtime(
                &mut command,
                &std::env::current_exe()?,
            )?;
            let deadline = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis()
                + 2000;
            command.args(["--exact", "installed::cleanup::tests::original_close_future_survives_drop_and_owner_cleanup"])
                .env(CHILD, directory.path()).env("VEOVEO_SMOKE_LOCAL_GROUPS", directory.path())
                .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.to_string())
                .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "4")
                .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            let mut child = veoveo_testing_support::ChildGuard::from_command(command)?;
            return tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(status) = child.try_wait()? {
                        ensure!(
                            status.success(),
                            "isolated Speech owner-close control failed"
                        );
                        ensure!(
                            std::fs::read(directory.path().join("settled"))?
                                == b"original-future-settled",
                            "isolated Speech owner-close control did not run"
                        );
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .context("isolated Speech owner-close control deadline")?;
        }
        let starts = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicBool::new(false));
        let work_dropped = Arc::new(AtomicBool::new(false));
        let retained = Arc::new(Mutex::new(Slot::default()));
        retained.lock().await.retain(());
        let result = owner::run(async {
            let slot = retained.clone();
            let dropped = work_dropped.clone();
            owner::register_cleanup(
                owner::CleanupKind::Remote,
                "Speech SDK native slot",
                "original-close",
                move || async move {
                    ensure!(
                        dropped.load(Ordering::SeqCst),
                        "work was not dropped before cleanup"
                    );
                    slot.lock()
                        .await
                        .close(
                            |_| async { anyhow::bail!("replacement close must not start") },
                            owner::cleanup_deadline()?,
                        )
                        .await
                },
            )?;
            struct Watch(Arc<AtomicBool>);
            impl Drop for Watch {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            let starts = starts.clone();
            let completed = completed.clone();
            let pending = async {
                let _watch = Watch(work_dropped.clone());
                retained
                    .lock()
                    .await
                    .close(
                        move |_| async move {
                            starts.fetch_add(1, Ordering::SeqCst);
                            // The actual owner's execution deadline drops this in-progress close.
                            tokio::time::sleep(Duration::from_millis(2500)).await;
                            completed.store(true, Ordering::SeqCst);
                            Ok(())
                        },
                        owner::cleanup_deadline()?,
                    )
                    .await
            };
            pending.await
        })
        .await;
        ensure!(
            result.is_err()
                && starts.load(Ordering::SeqCst) == 1
                && completed.load(Ordering::SeqCst)
        );
        ensure!(retained.lock().await.closed);

        let mut failed = Slot::default();
        failed.retain(());
        ensure!(
            failed
                .close(|_| std::future::pending(), Instant::now())
                .await
                .is_err()
        );
        ensure!(!failed.closed && failed.failed);
        ensure!(
            failed
                .close(
                    |_| async { Ok(()) },
                    Instant::now() + Duration::from_secs(10)
                )
                .await
                .is_err()
        );
        std::fs::write(
            std::path::PathBuf::from(std::env::var_os(CHILD).unwrap()).join("settled"),
            b"original-future-settled",
        )?;
        Ok(())
    }
}
