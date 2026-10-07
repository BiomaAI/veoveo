//! Parent-controlled admission before the configured command executes its payload.
use super::*;
use nix::libc;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, mpsc};

struct Pipes {
    ready_read: OwnedFd,
    ready_write: OwnedFd,
    release_read: OwnedFd,
    release_write: OwnedFd,
}
fn pipe() -> Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    // SAFETY: pipe2 initializes exactly two valid descriptors on success.
    ensure!(
        unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } == 0,
        "cannot create owned launch handshake: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: each successful descriptor is acquired exactly once.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

#[derive(Serialize)]
enum LaunchFormat {
    #[serde(rename = "veoveo.ai/smoke-launch/v1")]
    V1,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum LaunchState {
    Pending,
    Released,
    Unresolved,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchReceipt {
    format: LaunchFormat,
    launch_id: uuid::Uuid,
    state: LaunchState,
    group: Option<GroupRegistration>,
}
struct Supervisor {
    pipes: Arc<Pipes>,
    receipt_path: PathBuf,
    receipt: LaunchReceipt,
    transferred: bool,
}
impl Supervisor {
    fn publish(&self) -> Result<()> {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let staging = self.receipt_path.with_extension("staging");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&staging)?;
        file.write_all(&serde_json::to_vec(&self.receipt)?)?;
        file.sync_all()?;
        fs::rename(staging, &self.receipt_path)?;
        Ok(())
    }
    fn abort(&mut self) {
        // The helper owns another Arc, keeping these descriptors valid even when
        // its fork has not begun. The queued abort byte reaches that later fork.
        let _ = unsafe {
            libc::write(
                self.pipes.release_write.as_raw_fd(),
                [0_u8].as_ptr().cast(),
                1,
            )
        };
        if let Some(group) = &self.receipt.group {
            let _ = killpg(Pid::from_raw(group.pid as i32), Signal::SIGKILL);
        }
        self.receipt.state = LaunchState::Unresolved;
        let _ = self.publish();
    }
    fn settled(&mut self) -> Result<()> {
        fs::remove_file(&self.receipt_path)?;
        self.transferred = true;
        Ok(())
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        if !self.transferred {
            self.abort();
        }
    }
}

// Observe without reaping so the leader's identity remains reserved until every
// live member has drained. A failed bound leaves the launch/group receipt intact.
#[cfg(test)]
fn kill_and_reap(child: &mut Child, end: Instant) -> Result<()> {
    kill_and_reap_until(child, end, None)
}
fn kill_and_reap_until(
    child: &mut Child,
    end: Instant,
    cleanup: Option<&super::owner::CommandCleanup>,
) -> Result<()> {
    let end = || cleanup.map_or(end, |cleanup| cleanup.end());
    let pid = Pid::from_raw(child.id() as i32);
    let _ = killpg(pid, Signal::SIGKILL);
    loop {
        if cleanup.is_some() {
            ensure!(
                Instant::now() < end(),
                "aborted launch cleanup expired; identity retained"
            );
        }
        let observation = waitid(
            Id::Pid(pid),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        )?;
        if !matches!(observation, WaitStatus::StillAlive) && !live_group(child.id())? {
            // WNOWAIT proved the child is already waitable; this only reaps it.
            child.wait()?;
            return Ok(());
        }
        ensure!(
            Instant::now() < end(),
            "aborted launch group did not drain; identity retained"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

/// The helper owns Command::spawn while the supervisor admits the stopped child.
/// Cancellation aborts the handshake; the payload cannot run without a release byte.
pub(crate) fn spawn(
    command: Command,
    registration_root: Option<&Path>,
    deadline: Instant,
    cancelled: &std::sync::atomic::AtomicBool,
    grace: Duration,
) -> Result<(Child, Option<PathBuf>)> {
    spawn_with_publication(
        command,
        registration_root,
        None,
        deadline,
        cancelled,
        grace,
        None,
        None,
        || {},
    )
}

/// Caller cancellation supplements the enclosing owner's cancellation.
pub(crate) fn spawn_cancellable(
    command: Command,
    registration_root: Option<&Path>,
    deadline: Instant,
    cancelled: &std::sync::atomic::AtomicBool,
    caller_cancelled: &std::sync::atomic::AtomicBool,
    cleanup: Arc<super::owner::CommandCleanup>,
) -> Result<(Child, Option<PathBuf>)> {
    spawn_with_publication(
        command,
        registration_root,
        None,
        deadline,
        cancelled,
        Duration::from_secs(1),
        Some(caller_cancelled),
        Some(cleanup),
        || {},
    )
}

#[cfg(test)]
pub(crate) fn spawn_fixture(
    command: Command,
    root: &Path,
    deadline: Instant,
    cleanup: Arc<super::owner::CommandCleanup>,
) -> Result<(Child, Option<PathBuf>)> {
    spawn_with_publication(
        command,
        None,
        Some(root),
        deadline,
        &std::sync::atomic::AtomicBool::new(false),
        Duration::from_secs(1),
        None,
        Some(cleanup),
        || {},
    )
}

fn spawn_with_publication(
    mut command: Command,
    registration_root: Option<&Path>,
    receipt_root: Option<&Path>,
    deadline: Instant,
    cancelled: &std::sync::atomic::AtomicBool,
    grace: Duration,
    caller_cancelled: Option<&std::sync::atomic::AtomicBool>,
    cleanup: Option<Arc<super::owner::CommandCleanup>>,
    on_published: impl FnOnce(),
) -> Result<(Child, Option<PathBuf>)> {
    use std::sync::atomic::Ordering;
    let is_cancelled = || {
        cancelled.load(Ordering::Acquire)
            || caller_cancelled.is_some_and(|caller| caller.load(Ordering::Acquire))
    };
    ensure!(
        !is_cancelled() && Instant::now() < deadline,
        "launch cancelled before fork"
    );
    let (ready_read, ready_write) = pipe()?;
    let (release_read, release_write) = pipe()?;
    let pipes = Arc::new(Pipes {
        ready_read,
        ready_write,
        release_read,
        release_write,
    });
    let launch_id = uuid::Uuid::now_v7();
    let receipt_root = receipt_root
        .or(registration_root)
        .map(Path::to_path_buf)
        .or_else(|| {
            command
                .get_envs()
                .find(|(key, _)| *key == "VEOVEO_SMOKE_LOCAL_GROUPS")
                .and_then(|(_, value)| value)
                .map(PathBuf::from)
        });
    let receipt_root = match receipt_root {
        Some(root) => root,
        None => {
            use std::os::unix::fs::DirBuilderExt;
            let root = std::env::temp_dir().join(format!("veoveo-launch-{launch_id}"));
            fs::DirBuilder::new().mode(0o700).create(&root)?;
            root
        }
    };
    let mut supervisor = Supervisor {
        pipes: Arc::clone(&pipes),
        receipt_path: receipt_root.join(format!(".launch-{launch_id}.json")),
        receipt: LaunchReceipt {
            format: LaunchFormat::V1,
            launch_id,
            state: LaunchState::Pending,
            group: None,
        },
        transferred: false,
    };
    // Publish the launch intent before admitting the helper's fork. Even a slow
    // unresolved fork has an owner receipt without containing argv or secrets.
    supervisor.publish()?;
    let ready_write = pipes.ready_write.as_raw_fd();
    let release_read = pipes.release_read.as_raw_fd();
    let ready_read = pipes.ready_read.as_raw_fd();
    let release_write = pipes.release_write.as_raw_fd();
    command.process_group(0);
    // SAFETY: the child closure only calls async-signal-safe raw syscalls with
    // preallocated descriptors and stack buffers. It allocates and locks nothing.
    unsafe {
        command.pre_exec(move || {
            libc::close(ready_read);
            libc::close(release_write);
            let pid = libc::getpid().to_ne_bytes();
            if libc::write(ready_write, pid.as_ptr().cast(), pid.len()) != pid.len() as isize {
                return Err(std::io::Error::from_raw_os_error(libc::EIO));
            }
            let mut release = [0_u8; 1];
            loop {
                let count = libc::read(release_read, release.as_mut_ptr().cast(), 1);
                if count == 1 {
                    break;
                }
                if count < 0 && *libc::__errno_location() == libc::EINTR {
                    continue;
                }
                return Err(std::io::Error::from_raw_os_error(libc::ECANCELED));
            }
            libc::close(ready_write);
            libc::close(release_read);
            if release[0] != 1 {
                return Err(std::io::Error::from_raw_os_error(libc::ECANCELED));
            }
            Ok(())
        });
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    let helper_pipes = Arc::clone(&pipes);
    let helper_cleanup = cleanup.clone();
    let helper = thread::spawn(move || {
        // Keep every numeric descriptor live until fork/exec has resolved. An
        // aborted supervisor never closes/reuses a descriptor before a slow fork.
        let result = command.spawn();
        drop(helper_pipes);
        if let Err(failed) = sender.send(result) {
            if let Ok(mut child) = failed.0 {
                // The receiver may have expired while this helper was starting.
                // Settlement failure deliberately leaves the supervisor receipt.
                let _ =
                    kill_and_reap_until(&mut child, deadline + grace, helper_cleanup.as_deref());
            }
        }
    });
    let mut publication = Some(on_published);
    let mut registration = None;
    let mut released = false;
    let mut aborted = false;
    let mut pid = None;
    let mut failure = None;
    let mut abort_end = None;
    loop {
        if let Ok(result) = receiver.try_recv() {
            // Receiving its result does not authorize an unbounded join.
            let join_end = || {
                if aborted || is_cancelled() {
                    cleanup
                        .as_ref()
                        .map_or(deadline + grace, |cleanup| cleanup.end())
                } else {
                    deadline + grace
                }
            };
            while !helper.is_finished() && Instant::now() < join_end() {
                thread::sleep(Duration::from_millis(1));
            }
            ensure!(
                helper.is_finished(),
                "owned launch helper unresolved after result; launch receipt retained"
            );
            helper
                .join()
                .map_err(|_| anyhow::anyhow!("owned launch helper panicked"))?;
            match result {
                Ok(mut child)
                    if aborted
                        || cleanup.is_some() && (is_cancelled() || Instant::now() >= deadline) =>
                {
                    kill_and_reap_until(
                        &mut child,
                        abort_end.unwrap_or(deadline + grace),
                        cleanup.as_deref(),
                    )?;
                    remove_local_registration(registration.as_deref())?;
                    supervisor.settled()?;
                    if let Some(error) = failure {
                        return Err(error);
                    }
                    anyhow::bail!("launch cancelled before payload admission");
                }
                Ok(child) => {
                    supervisor.settled()?;
                    return Ok((child, registration));
                }
                Err(error) => {
                    if aborted || is_cancelled() {
                        ensure!(
                            cleanup
                                .as_ref()
                                .is_none_or(|cleanup| Instant::now() < cleanup.end()),
                            "late aborted launch outcome; identity retained"
                        );
                    }
                    remove_local_registration(registration.as_deref())?;
                    supervisor.settled()?;
                    return Err(error).context("owned launch failed before payload admission");
                }
            }
        }
        if !released && !aborted {
            let mut poll = libc::pollfd {
                fd: ready_read,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: poll receives one initialized descriptor; zero timeout.
            if unsafe { libc::poll(&mut poll, 1, 0) } > 0 && poll.revents & libc::POLLIN != 0 {
                let mut bytes = [0_u8; std::mem::size_of::<libc::pid_t>()];
                // SAFETY: fixed-size buffer and an open read endpoint.
                ensure!(
                    unsafe { libc::read(ready_read, bytes.as_mut_ptr().cast(), bytes.len()) }
                        == bytes.len() as isize,
                    "incomplete launch identity handshake"
                );
                let child_pid = libc::pid_t::from_ne_bytes(bytes) as u32;
                pid = Some(child_pid);
                let (leader, _, start_ticks) = identity(child_pid)?;
                ensure!(
                    leader == child_pid,
                    "launch child is not its process group leader"
                );
                supervisor.receipt.group = Some(GroupRegistration {
                    pid: child_pid,
                    start_ticks,
                });
                supervisor.publish()?;
                let admitted =
                    register_at(Some(registration_root.unwrap_or(&receipt_root)), child_pid)
                        .and_then(|path| {
                            if registration_root.is_none() {
                                // The supervisor leader is tracked separately from the
                                // nested groups that its owner must drain before exit.
                                let path = path.context("missing supervisor registration")?;
                                let retained =
                                    receipt_root.join(format!(".leader-{child_pid}.json"));
                                fs::rename(&path, &retained)?;
                                Ok(Some(retained))
                            } else {
                                Ok(path)
                            }
                        });
                match admitted {
                    Ok(path) => registration = path,
                    Err(error) => failure = Some(error),
                }
                if failure.is_none() {
                    publication.take().expect("one launch publication")();
                }
                // Check again after publication, immediately before release.
                if failure.is_none() && !is_cancelled() && Instant::now() < deadline {
                    ensure!(
                        unsafe { libc::write(release_write, [1_u8].as_ptr().cast(), 1) } == 1,
                        "payload admission release failed"
                    );
                    released = true;
                    supervisor.receipt.state = LaunchState::Released;
                    supervisor.publish()?;
                }
            }
        }
        if !aborted && (failure.is_some() || is_cancelled() || Instant::now() >= deadline) {
            // The abort byte also reaches a child that has not forked yet.
            let _ = unsafe { libc::write(release_write, [0_u8].as_ptr().cast(), 1) };
            aborted = true;
            abort_end = Some(cleanup.as_ref().map_or_else(
                || (Instant::now() + grace).min(deadline + grace),
                |cleanup| cleanup.end(),
            ));
            if let Some(pid) = pid {
                let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
            }
        }
        if aborted {
            if let Some(cleanup) = &cleanup {
                abort_end = Some(cleanup.end());
            }
        }
        if abort_end.is_some_and(|end| Instant::now() >= end) {
            // Do not report a successful join or cleanup for an unresolved launch.
            // The helper still owns its pipes; a late fork receives the abort byte.
            anyhow::bail!(
                "owned launch unresolved after abort; retained group/launch identity requires reconciliation"
            );
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    #[test]
    fn launch_cancellation_uses_original_grace_shorter_than_command_allowance() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("payload");
        let original =
            super::super::owner::test_scope(root.path().to_owned(), Duration::from_millis(30));
        let cleanup = super::super::owner::CommandCleanup::new(Some(original.clone()));
        let cancel_scope = original.clone();
        let cancellation = thread::spawn(move || {
            thread::sleep(Duration::from_millis(40));
            super::super::owner::test_cancel(&cancel_scope);
        });
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf forbidden > \"$MARKER\""])
            .env("MARKER", &marker);
        // Delayed pre_exec is a controlled admission seam, not a kernel stall.
        unsafe {
            command.pre_exec(|| {
                let delay = libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 300_000_000,
                };
                libc::nanosleep(&delay, std::ptr::null_mut());
                Ok(())
            });
        }
        let started = Instant::now();
        assert!(
            spawn_cancellable(
                command,
                Some(root.path()),
                original.deadline,
                &original.cancelled,
                &AtomicBool::new(false),
                cleanup
            )
            .is_err()
        );
        assert!(
            started.elapsed() < Duration::from_millis(180),
            "launch exceeded original 30ms grace"
        );
        cancellation.join().unwrap();
        assert!(fs::read_dir(root.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".launch-")
        }));
        thread::sleep(Duration::from_millis(350));
        assert!(!marker.exists(), "late admission released payload");
        assert!(
            fs::read_dir(root.path()).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".launch-")),
            "expired launch receipt was erased"
        );
    }

    #[test]
    fn payload_observes_complete_published_identity_before_it_can_write() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("payload");
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "test -s \"$LEASE/$$\" && grep -q startTicks \"$LEASE/$$\" && printf admitted > \"$MARKER\""])
            .env("LEASE", root.path()).env("MARKER", &marker);
        let (mut child, registration) = spawn(
            command,
            Some(root.path()),
            Instant::now() + Duration::from_secs(2),
            &AtomicBool::new(false),
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(fs::read(&marker).unwrap(), b"admitted");
        remove_local_registration(registration.as_deref()).unwrap();
    }
    #[test]
    fn aborted_group_drains_descendants_before_identity_is_reaped() {
        let root = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30 & wait"]);
        let (mut child, registration) = spawn(
            command,
            Some(root.path()),
            Instant::now() + Duration::from_secs(2),
            &AtomicBool::new(false),
            Duration::from_secs(1),
        )
        .unwrap();
        let pid = child.id();
        kill_and_reap(&mut child, Instant::now() + Duration::from_secs(1)).unwrap();
        assert!(!live_group(pid).unwrap());
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        // Registration removal follows confirmed group settlement, never SIGKILL alone.
        assert!(registration.as_ref().unwrap().exists());
        remove_local_registration(registration.as_deref()).unwrap();
    }
    #[test]
    fn cancellation_before_fork_cannot_execute_payload() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("payload");
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf forbidden > \"$MARKER\""])
            .env("MARKER", &marker);
        assert!(
            spawn(
                command,
                Some(root.path()),
                Instant::now() + Duration::from_secs(1),
                &AtomicBool::new(true),
                Duration::from_millis(50)
            )
            .is_err()
        );
        assert!(!marker.exists());
    }
    #[test]
    fn refused_registration_aborts_before_payload_release() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("payload");
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf forbidden > \"$MARKER\""])
            .env("MARKER", &marker);
        assert!(
            spawn(
                command,
                Some(&root.path().join("absent")),
                Instant::now() + Duration::from_secs(1),
                &AtomicBool::new(false),
                Duration::from_millis(100)
            )
            .is_err()
        );
        assert!(!marker.exists());
    }
    #[test]
    fn cancellation_during_registry_publication_blocks_the_actual_payload() {
        use std::sync::atomic::Ordering;
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("payload");
        let cancelled = AtomicBool::new(false);
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf forbidden > \"$MARKER\""])
            .env("MARKER", &marker);
        assert!(
            spawn_with_publication(
                command,
                Some(root.path()),
                None,
                Instant::now() + Duration::from_secs(1),
                &cancelled,
                Duration::from_millis(100),
                None,
                None,
                || cancelled.store(true, Ordering::Release)
            )
            .is_err()
        );
        assert!(!marker.exists());
    }
    #[test]
    fn slow_prefork_handshake_retains_intent_and_never_releases_payload() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("payload");
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf forbidden > \"$MARKER\""])
            .env("MARKER", &marker);
        // This test-only hook uses a raw async-signal-safe sleep before the gate.
        unsafe {
            command.pre_exec(|| {
                let delay = libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 200_000_000,
                };
                libc::nanosleep(&delay, std::ptr::null_mut());
                Ok(())
            });
        }
        assert!(
            spawn(
                command,
                Some(root.path()),
                Instant::now() + Duration::from_millis(20),
                &AtomicBool::new(false),
                Duration::from_millis(20)
            )
            .is_err()
        );
        assert!(fs::read_dir(root.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".launch-")
        }));
        thread::sleep(Duration::from_millis(250));
        assert!(!marker.exists());
    }
}
