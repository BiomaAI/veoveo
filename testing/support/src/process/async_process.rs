//! Async I/O over a command admitted by the same parent-controlled launch gate.
use super::*;
use std::time::Instant;
use tokio::io::AsyncReadExt;

pub struct AsyncChild {
    guard: ChildGuard,
    cleanup_owner: Option<std::sync::Arc<crate::lifecycle::owner::Active>>,
    pub stdin: Option<tokio::process::ChildStdin>,
    pub stdout: Option<tokio::process::ChildStdout>,
    pub stderr: Option<tokio::process::ChildStderr>,
}
impl AsyncChild {
    /// Stop and observe this admitted group within its original owner's cleanup cap.
    /// Cancellation may interrupt the wait; subsequent calls retain the first cap.
    pub async fn cleanup_until(&mut self, deadline: Instant) -> Result<std::process::ExitStatus> {
        let owner = self.cleanup_owner.clone();
        let end = owner
            .as_ref()
            .map_or(Instant::now(), |owner| deadline.min(owner.cleanup_end()));
        let cleanup = self.guard.explicit_cleanup.get_or_insert(ExplicitCleanup {
            owner: owner.clone(),
            end,
            failed: false,
        });
        cleanup.end = cleanup.end.min(end);
        let end = cleanup.end;
        anyhow::ensure!(!cleanup.failed, "owned child cleanup previously failed");
        let result = async {
            let owner = owner.context("owned child cleanup requires its original owner")?;
            let active = crate::lifecycle::owner::active()
                .context("owned child cleanup owner is no longer active")?;
            anyhow::ensure!(
                std::sync::Arc::ptr_eq(&owner, &active),
                "owned child cleanup owner was replaced"
            );
            anyhow::ensure!(Instant::now() < end, "owned child cleanup deadline expired");
            if let Some(status) = self.guard.settled {
                return Ok(status);
            }
            if let Err(error) = self.start_kill()
                && crate::lifecycle::live_group(self.guard.child.id())?
            {
                return Err(error);
            }
            // Yield after the signal so cancellation can retain this wait before reaping.
            tokio::task::yield_now().await;
            loop {
                let end = end.min(owner.cleanup_end());
                anyhow::ensure!(Instant::now() < end, "owned child cleanup deadline expired");
                let active = crate::lifecycle::owner::active()
                    .context("owned child cleanup owner is no longer active")?;
                anyhow::ensure!(
                    std::sync::Arc::ptr_eq(&owner, &active),
                    "owned child cleanup owner was replaced"
                );
                // Do not reap the group leader while an owned descendant lives.
                if !crate::lifecycle::live_group(self.guard.child.id())?
                    && let Some(status) = self.guard.observe()?
                {
                    return Ok(status);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        .await;
        if result.is_err() {
            self.guard
                .explicit_cleanup
                .as_mut()
                .expect("latched cleanup")
                .failed = true;
        }
        result
    }
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus> {
        loop {
            if let Some(cleanup) = &self.guard.abort_cleanup {
                cleanup.check_effect()?;
            } else {
                crate::lifecycle::owner::check_effect()?;
            }
            if let Some(status) = self.guard.observe()? {
                return Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    pub async fn wait_with_output(mut self) -> Result<Output> {
        drop(self.stdin.take());
        let stdout = self.stdout.take();
        let stderr = self.stderr.take();
        let read_stdout = async {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = stdout {
                pipe.read_to_end(&mut bytes).await?;
            }
            Ok::<_, anyhow::Error>(bytes)
        };
        let read_stderr = async {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = stderr {
                pipe.read_to_end(&mut bytes).await?;
            }
            Ok::<_, anyhow::Error>(bytes)
        };
        let (stdout, stderr, status) = tokio::try_join!(read_stdout, read_stderr, self.wait())?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
    pub fn start_kill(&mut self) -> Result<()> {
        nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(self.guard.child.id() as i32),
            nix::sys::signal::Signal::SIGKILL,
        )?;
        Ok(())
    }
}
pub fn spawn_async(command: tokio::process::Command) -> Result<AsyncChild> {
    let owner = crate::lifecycle::owner::active();
    let (child, registration) = crate::lifecycle::spawn_local(command.into_std())?;
    let guard = ChildGuard {
        child,
        registration,
        settled: None,
        owner_scope: false,
        abort_cleanup: None,
        explicit_cleanup: None,
        drain_on_drop: None,
    };
    from_guard(guard, owner)
}

pub(super) fn from_guard(
    mut guard: ChildGuard,
    owner: Option<std::sync::Arc<crate::lifecycle::owner::Active>>,
) -> Result<AsyncChild> {
    let stdin = guard
        .child
        .stdin
        .take()
        .map(tokio::process::ChildStdin::from_std)
        .transpose()?;
    let stdout = guard
        .child
        .stdout
        .take()
        .map(tokio::process::ChildStdout::from_std)
        .transpose()?;
    let stderr = guard
        .child
        .stderr
        .take()
        .map(tokio::process::ChildStderr::from_std)
        .transpose()?;
    Ok(AsyncChild {
        cleanup_owner: owner,
        guard,
        stdin,
        stdout,
        stderr,
    })
}

pub async fn output_async(
    mut command: tokio::process::Command,
    timeout: Duration,
) -> Result<Output> {
    let caller_deadline = Instant::now() + timeout;
    let scope = crate::lifecycle::owner::active();
    let deadline = scope
        .as_ref()
        .map_or(caller_deadline, |owner| caller_deadline.min(owner.deadline));
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let cleanup = crate::lifecycle::owner::CommandCleanup::new(scope.clone());
    let root = std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").map(PathBuf::from);
    struct PendingLaunch {
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        cleanup: std::sync::Arc<crate::lifecycle::owner::CommandCleanup>,
    }
    impl Drop for PendingLaunch {
        fn drop(&mut self) {
            self.cleanup.end();
            self.cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
    let pending = PendingLaunch {
        cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        cleanup: cleanup.clone(),
    };
    let cancelled = std::sync::Arc::clone(&pending.cancelled);
    let admitted_owner = scope.clone();
    // The blocking handoff owns the admitted child too: dropping its receiver
    // cannot leave a successfully launched process without forced cleanup.
    let launch = tokio::task::spawn_blocking(move || {
        let (child, registration) = crate::lifecycle::spawn_local_until(
            command.into_std(),
            deadline,
            &cancelled,
            scope,
            root.as_deref(),
            cleanup.clone(),
        )?;
        Ok::<_, anyhow::Error>(ChildGuard {
            child,
            registration,
            settled: None,
            owner_scope: false,
            abort_cleanup: Some(cleanup),
            explicit_cleanup: None,
            drain_on_drop: None,
        })
    });
    let launched = tokio::time::timeout_at(deadline.into(), launch)
        .await
        .context("owned asynchronous command exceeded its admission deadline")??;
    let child = from_guard(launched?, admitted_owner)?;
    let result = tokio::time::timeout_at(deadline.into(), child.wait_with_output())
        .await
        .context("owned asynchronous command exceeded its deadline")?;
    drop(pending);
    result
}

#[cfg(test)]
#[path = "explicit_cleanup_tests.rs"]
mod explicit_cleanup_tests;
