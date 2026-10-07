//! Async I/O over a command admitted by the same parent-controlled launch gate.
use super::*;
use std::time::Instant;
use tokio::io::AsyncReadExt;

pub struct AsyncChild {
    guard: ChildGuard,
    pub stdin: Option<tokio::process::ChildStdin>,
    pub stdout: Option<tokio::process::ChildStdout>,
    pub stderr: Option<tokio::process::ChildStderr>,
}
impl AsyncChild {
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
    let (child, registration) = crate::lifecycle::spawn_local(command.into_std())?;
    let guard = ChildGuard {
        child,
        registration,
        settled: None,
        owner_scope: false,
        abort_cleanup: None,
        drain_on_drop: None,
    };
    from_guard(guard)
}

pub(super) fn from_guard(mut guard: ChildGuard) -> Result<AsyncChild> {
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
            drain_on_drop: None,
        })
    });
    let launched = tokio::time::timeout_at(deadline.into(), launch)
        .await
        .context("owned asynchronous command exceeded its admission deadline")??;
    let child = from_guard(launched?)?;
    let result = tokio::time::timeout_at(deadline.into(), child.wait_with_output())
        .await
        .context("owned asynchronous command exceeded its deadline")?;
    drop(pending);
    result
}
