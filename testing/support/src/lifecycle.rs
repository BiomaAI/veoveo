//! Linux process ownership uses one execution deadline and one cleanup end.
use anyhow::{Context, Result, ensure};
use nix::{
    sys::{
        signal::{Signal, killpg},
        wait::{Id, WaitPidFlag, WaitStatus, waitid},
    },
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use std::os::unix::process::CommandExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct GroupRegistration {
    pid: u32,
    start_ticks: u64,
}
fn identity(pid: u32) -> Result<(u32, char, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let (_, tail) = stat
        .rsplit_once(')')
        .context("invalid Linux process identity")?;
    let fields: Vec<_> = tail.split_whitespace().collect();
    ensure!(fields.len() >= 20, "incomplete Linux process identity");
    Ok((
        fields[2].parse()?,
        fields[0].chars().next().context("missing process state")?,
        fields[19].parse()?,
    ))
}
pub(crate) fn live_group(pid: u32) -> Result<bool> {
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(member) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if member == pid {
            continue;
        }
        if let Ok((group, state, _)) = identity(member)
            && group == pid
            && state != 'Z'
            && state != 'X'
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn registered(root: &Path) -> Result<Vec<GroupRegistration>> {
    let mut groups = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with('.'))
        {
            continue;
        }
        let bytes = fs::read(&path)?;
        ensure!(bytes.len() <= 1024, "group registration too large");
        let group: GroupRegistration = serde_json::from_slice(&bytes)?;
        ensure!(
            path.file_name().and_then(|s| s.to_str()) == Some(group.pid.to_string().as_str()),
            "group registration filename mismatch"
        );
        if let Ok((leader, _, start)) = identity(group.pid) {
            ensure!(
                leader == group.pid && start == group.start_ticks,
                "local group identity changed; refuse recycled PID"
            );
            groups.push(group);
        } else {
            anyhow::bail!(
                "registered group leader disappeared before owned cleanup; reconciliation required"
            );
        }
    }
    Ok(groups)
}
fn registered_live(root: &Path) -> Result<bool> {
    for group in registered(root)? {
        let (_, state, _) = identity(group.pid)?;
        if !matches!(state, 'Z' | 'X') || live_group(group.pid)? {
            return Ok(true);
        }
    }
    Ok(false)
}
// These identities were admitted before the cleanup signal. An orphan reaped by
// PID1 may disappear during drain; disappearance never grants another signal.
fn captured_groups_live(groups: &[GroupRegistration]) -> Result<bool> {
    for group in groups {
        match identity(group.pid) {
            Ok((leader, state, start)) => {
                ensure!(
                    leader == group.pid && start == group.start_ticks,
                    "local group identity changed during drain; refuse recycled PID"
                );
                if !matches!(state, 'Z' | 'X') || live_group(group.pid)? {
                    return Ok(true);
                }
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                if live_group(group.pid)? {
                    return Ok(true);
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}
fn signal_registered(root: &Path, signal: Signal) -> Result<()> {
    for group in registered(root)? {
        let _ = killpg(Pid::from_raw(group.pid as i32), signal);
    }
    Ok(())
}
mod gate;
pub mod owner;
pub(crate) mod teardown;

pub struct OwnedProcess {
    child: Child,
    groups: PathBuf,
    registration: Option<PathBuf>,
    settled: bool,
}
impl OwnedProcess {
    pub fn spawn(
        mut command: Command,
        groups: &Path,
        deadline: Instant,
        cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<Self> {
        ensure!(
            cfg!(target_os = "linux"),
            "scenario execution requires the qualified Linux termination profile"
        );
        ensure!(groups.is_dir(), "local ownership lease is missing");
        let grace = command
            .get_envs()
            .find(|(key, _)| *key == "VEOVEO_SMOKE_CLEANUP_SECONDS")
            .and_then(|(_, value)| value)
            .and_then(|value| value.to_str())
            .map(str::parse::<u64>)
            .transpose()?
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(1));
        // The explicit supervisor lease also owns its leader receipt. Standalone
        // framework commands need not duplicate this internal environment wiring.
        command.env("VEOVEO_SMOKE_LOCAL_GROUPS", groups);
        let (child, registration) = gate::spawn(command, None, deadline, cancellation, grace)?;
        Ok(Self {
            child,
            groups: groups.to_owned(),
            registration,
            settled: false,
        })
    }
    pub fn finish(
        mut self,
        deadline: Duration,
        grace: Duration,
        cancellation: &std::sync::atomic::AtomicBool,
    ) -> Result<ExitStatus> {
        use std::sync::atomic::Ordering;
        let started = Instant::now();
        let mut cleanup_end = None;
        let pid = Pid::from_raw(self.child.id() as i32);
        loop {
            let observation = waitid(
                Id::Pid(pid),
                WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
            )?;
            match observation {
                WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _) => {
                    // Keep the leader unreaped while all owned group identities are settled.
                    let pending = registered(&self.groups)?;
                    let escaped = live_group(self.child.id())? || !pending.is_empty();
                    for group in &pending {
                        let _ = killpg(Pid::from_raw(group.pid as i32), Signal::SIGKILL);
                    }
                    let _ = killpg(pid, Signal::SIGKILL);
                    let drain_end = cleanup_end.unwrap_or_else(|| Instant::now() + grace);
                    while (live_group(self.child.id())? || captured_groups_live(&pending)?)
                        && Instant::now() < drain_end
                    {
                        thread::sleep(Duration::from_millis(20));
                    }
                    let drained = !live_group(self.child.id())? && !captured_groups_live(&pending)?;
                    ensure!(
                        drained,
                        "owned local process group did not drain; leader identity retained"
                    );
                    let status = self.child.wait()?;
                    remove_local_registration(self.registration.as_deref())?;
                    self.registration = None;
                    self.settled = true;
                    ensure!(
                        !escaped,
                        "owner exited with registered/live local children; remote settlement requires owner reconciliation"
                    );
                    ensure!(
                        cleanup_end.is_none(),
                        "execution cancelled; owner reconciliation/result is required"
                    );
                    ensure!(
                        !cancellation.load(Ordering::Acquire)
                            && Instant::now().duration_since(started) < deadline,
                        "execution cancelled or deadline expired; owner reconciliation is required"
                    );
                    return Ok(status);
                }
                WaitStatus::StillAlive => {}
                _ => anyhow::bail!("unsupported child observation"),
            }
            let now = Instant::now();
            if cleanup_end.is_none()
                && (now.duration_since(started) >= deadline || cancellation.load(Ordering::Acquire))
            {
                cleanup_end = Some(now + grace);
                let _ = killpg(pid, Signal::SIGINT);
                signal_registered(&self.groups, Signal::SIGINT)?;
            }
            if cleanup_end.is_some_and(|end| now >= end) {
                signal_registered(&self.groups, Signal::SIGKILL)?;
                let _ = killpg(pid, Signal::SIGKILL);
                // No wait beyond G: a signal does not prove termination. Drop
                // may reap an already-drained leader, otherwise its identity stays.
                anyhow::bail!(
                    "cleanup grace expired; forced local termination is failure and remote reconciliation remains required"
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if !self.settled {
            let _ = signal_registered(&self.groups, Signal::SIGKILL);
            let _ = killpg(Pid::from_raw(self.child.id() as i32), Signal::SIGKILL);
            if matches!(
                waitid(
                    Id::Pid(Pid::from_raw(self.child.id() as i32)),
                    WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
                ),
                Ok(WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _))
            ) && matches!(live_group(self.child.id()), Ok(false))
                && matches!(registered_live(&self.groups), Ok(false))
            {
                let _ = self.child.wait();
                let _ = remove_local_registration(self.registration.as_deref());
            }
            // Nonterminal ownership remains private for reconciliation. Drop
            // starts no new grace interval and performs no unbounded join.
        }
    }
}
/// A nested local leader registers before its side effects; unregister only after owned drain.
pub fn register_local_group(pid: u32) -> Result<Option<PathBuf>> {
    let Some(root) = std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS") else {
        return Ok(None);
    };
    register_at(Some(Path::new(&root)), pid)
}
fn register_at(root: Option<&Path>, pid: u32) -> Result<Option<PathBuf>> {
    let Some(root) = root else {
        return Ok(None);
    };
    ensure!(root.is_dir(), "local ownership lease is missing");
    let (group, _, start_ticks) = identity(pid)?;
    ensure!(
        group == pid,
        "registered process is not a local group leader"
    );
    let path = root.join(pid.to_string());
    let temporary = root.join(format!(".{pid}-{}", uuid::Uuid::now_v7()));
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(&GroupRegistration {
        pid,
        start_ticks,
    })?)?;
    file.sync_all()?;
    // Hard-link publication refuses an existing identity and exposes complete bytes.
    let result = fs::hard_link(&temporary, &path);
    fs::remove_file(&temporary)?;
    result?;
    Ok(Some(path))
}

/// Configured child launch uses the same owner cancellation and absolute deadline.
pub(crate) fn spawn_local(mut command: Command) -> Result<(Child, Option<PathBuf>)> {
    owner::check_effect()?;
    let root = std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").map(PathBuf::from);
    if let Some(scope) = owner::active() {
        command
            .env("VEOVEO_SMOKE_LOCAL_GROUPS", &scope.groups)
            .env(
                "VEOVEO_SMOKE_DEADLINE_UNIX_MS",
                scope.unix_end_ms.to_string(),
            )
            .env(
                "VEOVEO_SMOKE_CLEANUP_SECONDS",
                scope.grace.as_secs().to_string(),
            );
        gate::spawn(
            command,
            Some(&scope.groups),
            scope.deadline,
            &scope.cancelled,
            scope.grace,
        )
    } else {
        gate::spawn(
            command,
            root.as_deref(),
            Instant::now() + Duration::from_secs(30),
            &std::sync::atomic::AtomicBool::new(false),
            Duration::from_secs(1),
        )
    }
}
/// Owned asynchronous launch carries the caller's absolute deadline through admission.
pub(crate) fn spawn_local_until(
    mut command: Command,
    deadline: Instant,
    caller_cancelled: &std::sync::atomic::AtomicBool,
    scope: Option<std::sync::Arc<owner::Active>>,
    root: Option<&Path>,
    cleanup: std::sync::Arc<owner::CommandCleanup>,
) -> Result<(Child, Option<PathBuf>)> {
    if let Some(scope) = scope {
        command
            .env("VEOVEO_SMOKE_LOCAL_GROUPS", &scope.groups)
            .env(
                "VEOVEO_SMOKE_DEADLINE_UNIX_MS",
                scope.unix_end_ms.to_string(),
            )
            .env(
                "VEOVEO_SMOKE_CLEANUP_SECONDS",
                scope.grace.as_secs().to_string(),
            );
        gate::spawn_cancellable(
            command,
            Some(&scope.groups),
            deadline.min(scope.deadline),
            &scope.cancelled,
            caller_cancelled,
            cleanup,
        )
    } else {
        gate::spawn_cancellable(
            command,
            root,
            deadline,
            &std::sync::atomic::AtomicBool::new(false),
            caller_cancelled,
            cleanup,
        )
    }
}
#[cfg(test)]
pub(crate) fn spawn_fixture_until(
    command: Command,
    root: &Path,
    deadline: Instant,
    cleanup: std::sync::Arc<owner::CommandCleanup>,
) -> Result<(Child, Option<PathBuf>)> {
    gate::spawn_fixture(command, root, deadline, cleanup)
}
pub(crate) fn spawn_cleanup(command: Command) -> Result<(Child, Option<PathBuf>)> {
    let scope = owner::active().context("cleanup requires active owner operation")?;
    gate::spawn(
        command,
        Some(&scope.groups),
        scope.cleanup_end(),
        &std::sync::atomic::AtomicBool::new(false),
        Duration::ZERO,
    )
}
pub fn remove_local_registration(path: Option<&Path>) -> Result<()> {
    if let Some(path) = path {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn clean_owner_exit_requires_no_registered_or_live_children() {
        let lease = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exit 0"]);
        let status = OwnedProcess::spawn(
            command,
            lease.path(),
            Instant::now() + Duration::from_secs(2),
            &AtomicBool::new(false),
        )
        .unwrap()
        .finish(
            Duration::from_secs(2),
            Duration::from_secs(1),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(status.success());
    }

    #[test]
    fn owner_exit_with_surviving_local_child_fails_after_owned_drain() {
        let lease = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30 & exit 0"]);
        let error = OwnedProcess::spawn(
            command,
            lease.path(),
            Instant::now() + Duration::from_secs(2),
            &AtomicBool::new(false),
        )
        .unwrap()
        .finish(
            Duration::from_secs(2),
            Duration::from_secs(1),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner exited with registered/live local children")
        );
    }

    #[test]
    fn cancellation_is_failure_and_cannot_become_an_owner_success() {
        let lease = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "trap 'exit 0' INT; while :; do sleep 1; done"]);
        let started = Instant::now();
        let error = OwnedProcess::spawn(
            command,
            lease.path(),
            Instant::now() + Duration::from_secs(2),
            &AtomicBool::new(false),
        )
        .unwrap()
        .finish(
            Duration::from_secs(30),
            Duration::from_millis(200),
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(
            error.to_string().contains("execution cancelled")
                || error.to_string().contains("cleanup grace expired")
        );
    }

    #[test]
    fn abrupt_owner_exit_drains_registered_child_groups_and_still_fails() {
        const CHILD: &str = "VEOVEO_TEST_ABRUPT_OWNER";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = PathBuf::from(root);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime
                .block_on(owner::run(async {
                    let mut command = Command::new("/bin/sleep");
                    command.arg("30");
                    let child = crate::process::ChildGuard::from_command(command)?;
                    fs::write(root.join(".ready"), child.child_pid().to_string())?;
                    std::future::pending::<Result<()>>().await
                }))
                .unwrap();
            return;
        }
        let lease = tempfile::tempdir().unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "lifecycle::tests::abrupt_owner_exit_drains_registered_child_groups_and_still_fails", "--nocapture"])
            .env(CHILD, lease.path());
        let process = OwnedProcess::spawn(
            command,
            lease.path(),
            Instant::now() + Duration::from_secs(3),
            &AtomicBool::new(false),
        )
        .unwrap();
        let ready_end = Instant::now() + Duration::from_secs(2);
        while !lease.path().join(".ready").exists() {
            assert!(
                Instant::now() < ready_end,
                "owner did not publish its actual child registration"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let child_pid: u32 = fs::read_to_string(lease.path().join(".ready"))
            .unwrap()
            .parse()
            .unwrap();
        let (_, _, child_start) = identity(child_pid).unwrap();
        let registration: GroupRegistration =
            serde_json::from_slice(&fs::read(lease.path().join(child_pid.to_string())).unwrap())
                .unwrap();
        assert_eq!(
            (registration.pid, registration.start_ticks),
            (child_pid, child_start)
        );
        killpg(Pid::from_raw(process.child.id() as i32), Signal::SIGKILL).unwrap();
        let error = process
            .finish(
                Duration::from_secs(2),
                Duration::from_secs(1),
                &AtomicBool::new(false),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner exited with registered/live local children"),
            "unexpected supervisor failure: {error:#}"
        );
        assert!(!live_group(child_pid).unwrap());
        if let Ok((_, state, start)) = identity(child_pid) {
            assert_eq!(
                start, child_start,
                "child identity was recycled during settlement"
            );
            assert!(
                matches!(state, 'Z' | 'X'),
                "registered child survived owner drain"
            );
        }
    }

    #[test]
    fn elapsed_execution_deadline_cannot_become_successful_teardown() {
        let lease = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "trap 'exit 0' INT; while :; do sleep 1; done"]);
        let started = Instant::now();
        let error = OwnedProcess::spawn(
            command,
            lease.path(),
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
        )
        .unwrap()
        .finish(
            Duration::from_millis(20),
            Duration::from_millis(100),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            error.to_string().contains("cancelled") || error.to_string().contains("grace expired")
        );
    }

    #[test]
    fn changed_registration_identity_is_rejected_before_signal() {
        let lease = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let (_, _, ticks) = identity(pid).unwrap();
        fs::write(
            lease.path().join(pid.to_string()),
            serde_json::to_vec(&GroupRegistration {
                pid,
                start_ticks: ticks + 1,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(registered(lease.path()).is_err());
        assert!(
            captured_groups_live(&[GroupRegistration {
                pid,
                start_ticks: ticks + 1
            }])
            .is_err()
        );
    }
}
