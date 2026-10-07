//! Preparation and owner execution consume one deadline; cancellation starts one cleanup grace.
use anyhow::{Context, Result, ensure};
use std::{
    cell::Cell,
    path::PathBuf,
    process::{Command, ExitStatus, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use veoveo_testing_support::lifecycle::OwnedProcess;
pub(crate) struct Budget {
    end: Instant,
    target_root: PathBuf,
    started: Instant,
    started_unix_ms: u128,
    deadline_unix_ms: u128,
    grace: Duration,
    cancellation: Arc<AtomicBool>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    observer: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    directory: tempfile::TempDir,
    captures: Cell<u64>,
    retained: Cell<bool>,
}
impl Budget {
    pub(crate) fn new(target: &std::path::Path, deadline: u64, cleanup: u64) -> Result<Self> {
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
        let deadline_unix_ms = started
            .as_millis()
            .checked_add(u128::from(deadline) * 1000)
            .context("wall deadline overflow")?;
        let start = Instant::now();
        let end = start
            .checked_add(Duration::from_secs(deadline))
            .context("deadline overflow")?;
        std::fs::create_dir_all(target)?;
        let mut directory = tempfile::tempdir_in(target)?;
        directory.disable_cleanup(true);
        std::fs::create_dir(directory.path().join("groups"))?;
        let cancellation = Arc::new(AtomicBool::new(false));
        let flag = cancellation.clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let (ready, registered) = std::sync::mpsc::sync_channel(0);
        let observer = std::thread::spawn(move || {
            runtime.block_on(async move {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
            let _ = ready.send(());
            tokio::select! { _ = term.recv() => {}, _ = interrupt.recv() => {}, _ = stopped => return Ok(()) }
            flag.store(true, Ordering::Release);
            Ok(())
        })
        });
        registered
            .recv_timeout(end.saturating_duration_since(Instant::now()))
            .context(
                "signal observer failed or exceeded the preparation deadline during registration",
            )?;
        Ok(Self {
            end,
            target_root: target.canonicalize()?,
            started: start,
            started_unix_ms: started.as_millis(),
            deadline_unix_ms,
            grace: Duration::from_secs(cleanup),
            cancellation,
            stop: Some(stop),
            observer: Some(observer),
            directory,
            captures: Cell::new(0),
            retained: Cell::new(true),
        })
    }
    pub(crate) fn select(&mut self, deadline: u64, cleanup: u64) -> Result<()> {
        self.end = self
            .started
            .checked_add(Duration::from_secs(deadline))
            .context("deadline overflow")?;
        self.deadline_unix_ms = self
            .started_unix_ms
            .checked_add(u128::from(deadline) * 1000)
            .context("deadline overflow")?;
        self.grace = Duration::from_secs(cleanup);
        ensure!(
            !self.cancellation.load(Ordering::Acquire) && Instant::now() < self.end,
            "cancelled or expired during descriptor admission"
        );
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn cancel_for_test(&self) {
        self.cancellation.store(true, Ordering::Release);
    }
    pub(crate) fn retain(&self) {
        self.retained.set(true);
    }
    pub(crate) fn completed(&self) {
        self.retained.set(false);
    }
    pub(crate) fn path(&self) -> &std::path::Path {
        self.directory.path()
    }
    pub(crate) fn groups(&self) -> PathBuf {
        self.path().join("groups")
    }
    pub(crate) fn finish(&self, mut command: Command) -> Result<ExitStatus> {
        ensure!(
            !self.cancellation.load(Ordering::Acquire),
            "execution cancelled before next preparation/effect"
        );
        ensure!(
            Instant::now() < self.end,
            "execution deadline exhausted before next preparation/effect"
        );
        command
            .env("CARGO_TARGET_DIR", &self.target_root)
            .env(
                "VEOVEO_SMOKE_DEADLINE_UNIX_MS",
                self.deadline_unix_ms.to_string(),
            )
            .env(
                "VEOVEO_SMOKE_CLEANUP_SECONDS",
                self.grace.as_secs().to_string(),
            );
        let process = OwnedProcess::spawn(command, &self.groups(), self.end, &self.cancellation)?;
        let remaining = self
            .end
            .checked_duration_since(Instant::now())
            .context("execution deadline exhausted during bounded startup")?;
        process.finish(remaining, self.grace, &self.cancellation)
    }
    pub(crate) fn output(&self, mut command: Command) -> Result<Output> {
        let number = self.captures.get();
        self.captures.set(number + 1);
        let stdout = self.path().join(format!("preparation-{number}.stdout"));
        let stderr = self.path().join(format!("preparation-{number}.stderr"));
        command
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&stdout)?)
            .stderr(std::fs::File::create(&stderr)?);
        let status = self.finish(command).with_context(|| {
            format!(
                "preparation diagnostics retained at {}",
                self.path().display()
            )
        })?;
        ensure!(
            status.success(),
            "preparation failed with {status}; private diagnostics retained at {}",
            self.path().display()
        );
        Ok(Output {
            status,
            stdout: std::fs::read(stdout)?,
            stderr: std::fs::read(stderr)?,
        })
    }
}
impl Drop for Budget {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(observer) = self.observer.take() {
            let _ = observer.join();
        }
        self.directory.disable_cleanup(self.retained.get());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_preparation_identifies_retained_private_diagnostics() {
        let root = tempfile::tempdir().unwrap();
        let budget = Budget::new(root.path(), 5, 1).unwrap();
        let retained = budget.path().to_owned();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf 'owned failure diagnostic' >&2; exit 23"]);
        let error = budget.output(command).unwrap_err().to_string();
        assert!(error.contains("23"), "original exit status: {error}");
        assert!(
            error.contains(&retained.display().to_string()),
            "retained directory must be actionable: {error}"
        );
        drop(budget);
        assert_eq!(
            std::fs::read_to_string(retained.join("preparation-0.stderr")).unwrap(),
            "owned failure diagnostic"
        );
    }
    #[test]
    fn preparations_and_execution_share_deadline_and_cancel_prevents_next_effect() {
        let root = tempfile::tempdir().unwrap();
        let budget = Budget::new(root.path(), 1, 1).unwrap();
        let mut first = Command::new("/bin/sh");
        first.args(["-c", "sleep 0.7"]);
        budget.output(first).unwrap();
        let mut second = Command::new("/bin/sh");
        second.args(["-c", "sleep 0.7"]);
        assert!(
            budget.output(second).is_err(),
            "preparation must not reset D"
        );
        let budget = Budget::new(root.path(), 5, 1).unwrap();
        budget.cancellation.store(true, Ordering::Release);
        let sentinel = root.path().join("unexpected-effect");
        let mut command = Command::new("touch");
        command.arg(&sentinel);
        assert!(budget.output(command).is_err());
        assert!(
            !sentinel.exists(),
            "cancelled preparation must not start another process"
        );
    }
}
