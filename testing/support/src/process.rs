use super::*;
use std::time::Duration;

pub struct ChildGuard {
    child: Child,
    drain_on_drop: Option<Duration>,
    registration: Option<PathBuf>,
    settled: Option<std::process::ExitStatus>,
    owner_scope: bool,
    abort_cleanup: Option<std::sync::Arc<crate::lifecycle::owner::CommandCleanup>>,
}
impl ChildGuard {
    pub fn spawn(
        program: &Path,
        args: impl IntoIterator<Item = OsString>,
        envs: impl IntoIterator<Item = (&'static str, OsString)>,
        log: &Path,
    ) -> Result<Self> {
        anyhow::ensure!(
            cfg!(target_os = "linux"),
            "owner child execution requires Linux process ownership"
        );
        crate::lifecycle::owner::check_effect()?;
        let stdout = File::create(log)
            .with_context(|| format!("create private child log {}", log.display()))?;
        let stderr = stdout.try_clone()?;
        let mut command = Command::new(program);
        configure_binary_runtime(&mut command, program)?;
        command
            .args(args)
            .envs(envs)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        let (child, registration) = crate::lifecycle::spawn_local(command)?;
        Ok(Self {
            child,
            registration,
            settled: None,
            owner_scope: false,
            abort_cleanup: None,
            drain_on_drop: None,
        })
    }
    /// Consume the configured command without replacing its stdio, environment or cwd.
    pub fn from_command(command: Command) -> Result<Self> {
        crate::lifecycle::owner::check_effect()?;
        let (child, registration) = crate::lifecycle::spawn_local(command)?;
        Ok(Self {
            child,
            registration,
            settled: None,
            owner_scope: false,
            abort_cleanup: None,
            drain_on_drop: None,
        })
    }
    #[cfg(test)]
    pub(crate) fn child_pid(&self) -> u32 {
        self.child.id()
    }
    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        self.observe()
    }
    pub fn with_owner_scope(mut self) -> Self {
        self.owner_scope = true;
        self
    }
    pub fn with_drain_on_drop(mut self, timeout: Duration) -> Self {
        self.drain_on_drop = Some(timeout);
        self
    }
    fn observe(&mut self) -> Result<Option<std::process::ExitStatus>> {
        use nix::{
            sys::wait::{Id, WaitPidFlag, WaitStatus, waitid},
            unistd::Pid,
        };
        if let Some(status) = self.settled {
            return Ok(Some(status));
        }
        match waitid(
            Id::Pid(Pid::from_raw(self.child.id() as i32)),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        )? {
            WaitStatus::StillAlive => Ok(None),
            WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _) => {
                anyhow::ensure!(
                    !crate::lifecycle::live_group(self.child.id())?,
                    "child exited with owned descendants still active"
                );
                // The unreaped leader reserves its process-group identity until owned drain.
                let status = self.child.wait()?;
                crate::lifecycle::remove_local_registration(self.registration.as_deref())?;
                self.registration = None;
                self.settled = Some(status);
                Ok(Some(status))
            }
            _ => anyhow::bail!("unsupported child observation"),
        }
    }
    pub fn stop(&mut self) {
        if self.settled.is_some() {
            return;
        }
        if let Some(cleanup) = self.abort_cleanup.clone() {
            // The unreaped leader reserves this group identity. Async command
            // cancellation cannot spend the surrounding scenario's execution budget.
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(self.child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            while std::time::Instant::now() < cleanup.end() {
                if matches!(self.observe(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            // Unresolved process/group registrations remain for owner reconciliation.
            return;
        }
        use nix::{
            sys::signal::{Signal, killpg},
            unistd::Pid,
        };
        let pid = Pid::from_raw(self.child.id() as i32);
        if let Some(timeout) = self.drain_on_drop.or(Some(Duration::from_secs(20))) {
            let _ = killpg(pid, Signal::SIGINT);
            let end = crate::lifecycle::owner::active()
                .map(|scope| scope.cleanup_end())
                .unwrap_or_else(|| std::time::Instant::now() + timeout);
            while std::time::Instant::now() < end {
                match self.observe() {
                    Ok(Some(_)) => return,
                    Ok(None) => {}
                    Err(_) => break,
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let _ = killpg(pid, Signal::SIGKILL);
        let end = crate::lifecycle::owner::active()
            .map(|scope| scope.cleanup_end())
            .unwrap_or_else(|| std::time::Instant::now() + Duration::from_secs(5));
        while std::time::Instant::now() < end {
            match self.observe() {
                Ok(Some(_)) => return,
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        // Preserve registration on failed local drain; the dispatcher refuses completion.
    }
    pub async fn stop_checked(&mut self) -> Result<()> {
        let timeout = crate::lifecycle::owner::active()
            .map(|scope| scope.grace)
            .unwrap_or(Duration::from_secs(20));
        self.drain(timeout).await
    }
    pub async fn drain(&mut self, timeout: Duration) -> Result<()> {
        if let Some(status) = self.observe()? {
            anyhow::ensure!(
                status.success(),
                "child exited before requested shutdown: {status}"
            );
            return Ok(());
        }
        let scope = crate::lifecycle::owner::active()
            .context("checked child shutdown requires an active owner")?;
        let requested = if self.owner_scope {
            Some(crate::lifecycle::teardown::request(
                &scope.groups,
                self.child.id(),
            )?)
        } else {
            None
        };
        if requested.is_none() {
            nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(self.child.id() as i32),
                nix::sys::signal::Signal::SIGINT,
            )?;
        }
        let end = scope.cleanup_end().min(std::time::Instant::now() + timeout);
        loop {
            if let Some(status) = self.observe()? {
                if let Some(request) = &requested {
                    crate::lifecycle::teardown::verify(&scope.groups, request)?;
                } else {
                    anyhow::ensure!(status.success(), "child shutdown failed: {status}");
                }
                return Ok(());
            }
            anyhow::ensure!(
                std::time::Instant::now() < end,
                "child shutdown unresolved at original cleanup end"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct ContainerGuard {
    name: String,
    created: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    cid_file: PathBuf,
    cleanup: crate::lifecycle::owner::CleanupRegistration,
}
impl ContainerGuard {
    /// Publish dispatch intent before docker run; a name alone never authorizes deletion.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        anyhow::ensure!(
            !name.is_empty() && name.len() <= 128,
            "invalid created fixture name"
        );
        let created = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        let identity = std::sync::Arc::clone(&created);
        let root = env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir);
        let directory = root.join(format!(".container-{}", uuid::Uuid::now_v7()));
        fs::create_dir(&directory)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        let cid_file = directory.join("created.cid");
        let observed_cid = cid_file.clone();
        let cleanup = crate::lifecycle::owner::register_cleanup(
            crate::lifecycle::owner::CleanupKind::Container,
            "docker_fixture",
            &name,
            move || async move {
                let id = match identity.lock().expect("created container identity").clone() {
                    Some(id) => id,
                    None => admitted_container_id(&fs::read_to_string(&observed_cid).context(
                        "container creation outcome unresolved; refuse name-based deletion",
                    )?)?,
                };
                cleanup_container(&id).await?;
                *identity.lock().expect("created container identity") = None;
                fs::remove_dir_all(
                    observed_cid
                        .parent()
                        .context("missing private container lease")?,
                )?;
                Ok(())
            },
        )?;
        Ok(Self {
            name,
            created,
            cid_file,
            cleanup,
        })
    }
    /// Only stdout of this owner's successful docker run admits the created identity.
    pub fn record_created(&mut self, stdout: &str) -> Result<()> {
        let id = admitted_container_id(stdout)?;
        let recorded = admitted_container_id(
            &fs::read_to_string(&self.cid_file)
                .context("created fixture CID file is missing; identity unresolved")?,
        )?;
        anyhow::ensure!(
            id == recorded,
            "created fixture stdout and CID identity disagree"
        );
        let mut created = self.created.lock().expect("created container identity");
        anyhow::ensure!(
            created.is_none(),
            "container creation identity already admitted"
        );
        self.cleanup.observed_identity(&id)?;
        *created = Some(id);
        Ok(())
    }
    /// Confirm removal of this invocation's created identity before reporting success.
    pub async fn cleanup(&mut self) -> Result<()> {
        let admitted = self
            .created
            .lock()
            .expect("created container identity")
            .clone();
        let id = match admitted {
            Some(id) => id,
            None => admitted_container_id(
                &fs::read_to_string(&self.cid_file)
                    .context("created container identity unresolved")?,
            )?,
        };
        cleanup_container(&id).await?;
        *self.created.lock().expect("created container identity") = None;
        fs::remove_dir_all(
            self.cid_file
                .parent()
                .context("missing private container lease")?,
        )?;
        self.cleanup.settled()?;
        Ok(())
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn cid_file(&self) -> &Path {
        &self.cid_file
    }
    pub async fn wait_created(&mut self) -> Result<()> {
        loop {
            crate::lifecycle::owner::check_effect()?;
            match fs::read_to_string(&self.cid_file) {
                Ok(id) => return self.record_created(&id),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
fn admitted_container_id(text: &str) -> Result<String> {
    let id = text.trim();
    anyhow::ensure!(
        id.len() == 64
            && id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "Docker creation did not supply an admitted container ID"
    );
    Ok(id.to_owned())
}
async fn cleanup_container(id: &str) -> Result<()> {
    let mut command = Command::new("docker");
    command
        .args(["rm", "-f", "--volumes", id])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let (child, registration) = crate::lifecycle::spawn_cleanup(command)?;
    let mut guard = ChildGuard {
        child,
        registration,
        drain_on_drop: Some(Duration::ZERO),
        settled: None,
        owner_scope: false,
        abort_cleanup: None,
    };
    let scope = crate::lifecycle::owner::active().context("missing fixture cleanup owner")?;
    let end = scope.cleanup_end();
    loop {
        if let Some(status) = guard.observe()? {
            anyhow::ensure!(
                status.success(),
                "created Docker fixture removal refused; identity retained"
            );
            return Ok(());
        }
        anyhow::ensure!(
            std::time::Instant::now() < end,
            "created Docker fixture removal unresolved; identity retained"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
impl Drop for ContainerGuard {
    fn drop(&mut self) {
        // The active owner awaits the registered action within its original G.
        // Once that owner is gone, Drop cannot extend cleanup authority or start
        // another mutation. The private CID/receipt remains for reconciliation.
    }
}

pub fn assert_executable(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("required binary does not exist: {}", path.display());
    }
    Ok(())
}

pub fn run_checked(
    program: &Path,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let output = run_raw(program, args, envs)?;
    if !output.status.success() {
        bail!(
            "{} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub fn run_raw(
    program: &Path,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<Output> {
    let mut command = Command::new(program);
    configure_binary_runtime(&mut command, program)?;
    crate::lifecycle::owner::check_effect()?;
    command
        .args(args)
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_ID")
        .env_remove("VEOVEO_INTERNAL_TRUST_JWKS")
        .env_remove("VEOVEO_AUDIT_SIGNING_KEY_B64")
        .env_remove("VEOVEO_AUDIT_RETENTION_DAYS")
        .env_remove("VEOVEO_REFRESH_DELIVERY_KEY_B64")
        .env_remove("VEOVEO_REFRESH_DELIVERY_WINDOW_SECONDS")
        .envs(envs);
    output_command(command)
}

/// Capture the configured command through the same admitted local ownership gate.
pub fn output_command(command: Command) -> Result<Output> {
    output_command_in_owner(command, false, true)
}

/// Reconcile an already registered cleanup identity during its original grace interval.
pub fn output_cleanup_command(command: Command) -> Result<Output> {
    output_command_in_owner(command, true, true)
}

/// Preserve the command's explicit stdout destination while capturing its diagnostics.
pub fn output_command_with_configured_stdout(command: Command, cleanup: bool) -> Result<Output> {
    output_command_in_owner(command, cleanup, false)
}

fn output_command_in_owner(
    mut command: Command,
    cleanup: bool,
    capture_stdout: bool,
) -> Result<Output> {
    use std::os::unix::fs::OpenOptionsExt;
    let root = env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let capture = root.join(format!(".command-output-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&capture)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&capture, fs::Permissions::from_mode(0o700))?;
    let stdout = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(capture.join("stdout"))?;
    let stderr = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(capture.join("stderr"))?;
    if capture_stdout {
        command.stdout(stdout);
    }
    command.stderr(stderr);
    let (child, registration) = if cleanup {
        crate::lifecycle::spawn_cleanup(command)?
    } else {
        crate::lifecycle::owner::check_effect()?;
        crate::lifecycle::spawn_local(command)?
    };
    let mut guard = ChildGuard {
        child,
        registration,
        settled: None,
        owner_scope: false,
        abort_cleanup: None,
        drain_on_drop: Some(Duration::from_secs(1)),
    };
    let end = crate::lifecycle::owner::active()
        .map(|scope| {
            if cleanup {
                scope.cleanup_end()
            } else {
                scope.deadline
            }
        })
        .unwrap_or_else(|| std::time::Instant::now() + Duration::from_secs(300));
    let status = loop {
        if let Some(status) = guard.observe()? {
            break status;
        }
        if !cleanup {
            crate::lifecycle::owner::check_effect()?;
        }
        anyhow::ensure!(
            std::time::Instant::now() < end,
            "command execution deadline expired; private output retained"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: if capture_stdout {
            fs::read(capture.join("stdout"))?
        } else {
            Vec::new()
        },
        stderr: fs::read(capture.join("stderr"))?,
    };
    fs::remove_dir_all(&capture)?;
    Ok(output)
}

pub fn configure_binary_runtime(command: &mut Command, program: &Path) -> Result<()> {
    let Some(path) = env::var_os("VEOVEO_SMOKE_ARTIFACTS") else {
        return Ok(());
    };
    let manifest: crate::artifacts::ArtifactManifest = serde_json::from_slice(&fs::read(path)?)?;
    manifest.admit(&manifest.repository)?;
    // Owner-supplied system/provider programs remain distinct from Cargo artifacts.
    if !program.is_absolute() {
        return Ok(());
    }
    let canonical = program.canonicalize()?;
    let entry = manifest.entries.iter().find(|entry| {
        entry
            .executable
            .canonicalize()
            .is_ok_and(|path| path == canonical)
    });
    match entry {
        Some(entry) => crate::artifacts::configure_runtime(command, entry),
        None => {
            anyhow::ensure!(
                !canonical.starts_with(manifest.target_root.canonicalize()?),
                "Cargo child is absent from admitted artifact manifest"
            );
            Ok(())
        }
    }
}

pub fn prepend_path_env(command: &mut Command, key: &str, path: &Path) {
    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os(key) {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        command.env(key, joined);
    }
}

pub fn smoke_tmpdir() -> Result<PathBuf> {
    let tmpdir = env::temp_dir().join(format!("veoveo-smoke-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmpdir)?;
    Ok(tmpdir)
}

pub struct TmpDirGuard {
    path: PathBuf,
    remove_on_drop: bool,
}

impl TmpDirGuard {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            remove_on_drop: false,
        }
    }

    pub fn remove_on_drop(&mut self) {
        self.remove_on_drop = true;
    }
}

impl Drop for TmpDirGuard {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = std::fs::remove_dir_all(&self.path);
        } else {
            eprintln!(
                "smoke failed; leaving workspace for logs: {}",
                self.path.display()
            );
        }
    }
}

mod async_process;
pub use async_process::{AsyncChild, output_async, spawn_async};

#[cfg(test)]
mod tests {
    use super::*;

    // These nested libtest fixtures own their command environment and lease.
    // A concurrent test's global ACTIVE scope cannot admit or configure them.
    fn fixture_child(command: Command) -> ChildGuard {
        let root = command
            .get_envs()
            .find(|(key, _)| *key == "VEOVEO_SMOKE_LOCAL_GROUPS")
            .and_then(|(_, value)| value)
            .map(PathBuf::from)
            .expect("private fixture lease");
        let cleanup = crate::lifecycle::owner::CommandCleanup::new(None);
        let (child, registration) = crate::lifecycle::spawn_fixture_until(
            command,
            &root,
            std::time::Instant::now() + Duration::from_secs(6),
            std::sync::Arc::clone(&cleanup),
        )
        .unwrap();
        ChildGuard {
            child,
            registration,
            settled: None,
            owner_scope: false,
            abort_cleanup: Some(cleanup),
            drain_on_drop: Some(Duration::ZERO),
        }
    }

    fn isolated_control(name: &str, key: &str, mode: &str) {
        let root = tempfile::tempdir().unwrap();
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .args(["--exact", name, "--nocapture"])
            .env(key, mode)
            .env("VEOVEO_SMOKE_LOCAL_GROUPS", root.path());
        let mut child = fixture_child(command);
        let end = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "isolated {mode} control failed");
                return;
            }
            assert!(
                std::time::Instant::now() < end,
                "isolated control timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn private_fixture_lease_and_failed_cleanup_ignore_dispatcher_context() {
        const KEY: &str = "VEOVEO_TEST_PRIVATE_LEASE";
        if env::var_os(KEY).is_some() {
            let foreign = PathBuf::from(env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            let private = tempfile::tempdir().unwrap();
            let other =
                crate::lifecycle::owner::test_scope(foreign.clone(), Duration::from_millis(5));
            crate::lifecycle::owner::test_cancel(&other);
            std::thread::sleep(Duration::from_millis(10));
            crate::lifecycle::owner::test_activate(Some(&other));
            let marker = private.path().join(".ready");
            let mut command = Command::new("/bin/sh");
            command
                .args([
                    "-c",
                    "trap '' INT TERM; sleep 30 & printf ready > \"$MARKER\"; exit 1",
                ])
                .env("MARKER", &marker)
                .env("VEOVEO_SMOKE_LOCAL_GROUPS", private.path());
            let mut child = fixture_child(command);
            let pid = child.child.id();
            let registration = child.registration.clone().unwrap();
            assert_eq!(registration.parent().unwrap(), private.path());
            let end = std::time::Instant::now() + Duration::from_secs(1);
            while !marker.exists() {
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(Duration::from_millis(5));
            }
            std::thread::sleep(Duration::from_millis(20));
            assert!(
                child.try_wait().is_err(),
                "failed fixture with survivor passed"
            );
            drop(child);
            assert!(!crate::lifecycle::live_group(pid).unwrap());
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
            assert!(
                !registration.exists(),
                "settled private fixture retained registration"
            );
            assert!(
                !fs::read_dir(&foreign).unwrap().any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .parse::<u32>()
                    .is_ok()),
                "fixture leaked into dispatcher registry"
            );
            crate::lifecycle::owner::test_activate(None);
            return;
        }
        isolated_control(
            "process::tests::private_fixture_lease_and_failed_cleanup_ignore_dispatcher_context",
            KEY,
            "failed",
        );
    }

    #[test]
    fn late_async_handoff_keeps_original_cleanup_end_and_registration() {
        const KEY: &str = "VEOVEO_TEST_LATE_HANDOFF";
        if let Ok(mode) = env::var(KEY) {
            let root = tempfile::tempdir().unwrap();
            let original = crate::lifecycle::owner::test_scope(
                root.path().to_owned(),
                if mode == "caller_end" {
                    Duration::from_secs(5)
                } else {
                    Duration::from_millis(30)
                },
            );
            let cleanup = crate::lifecycle::owner::CommandCleanup::new(Some(original.clone()));
            let mut command = Command::new("/bin/sh");
            let marker = root.path().join(".ready");
            command
                .args([
                    "-c",
                    "trap '' INT TERM; sleep 30 & printf ready > \"$MARKER\"; wait",
                ])
                .env("MARKER", &marker);
            let (child, registration) = crate::lifecycle::spawn_local_until(
                command,
                std::time::Instant::now() + Duration::from_secs(2),
                &std::sync::atomic::AtomicBool::new(false),
                Some(original.clone()),
                None,
                cleanup.clone(),
            )
            .unwrap();
            let pid = child.id();
            let registration = registration.unwrap();
            let end = std::time::Instant::now() + Duration::from_secs(1);
            while !marker.exists() {
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(Duration::from_millis(5));
            }
            // A deterministic handoff barrier holds the actual launched guard.
            let (release, wait) = std::sync::mpsc::channel();
            let (send, receive) = std::sync::mpsc::channel();
            let command_first = cleanup.end();
            let helper_cleanup = cleanup.clone();
            let helper = std::thread::spawn(move || {
                let guard = ChildGuard {
                    child,
                    registration: Some(registration),
                    settled: None,
                    owner_scope: false,
                    abort_cleanup: Some(helper_cleanup),
                    drain_on_drop: None,
                };
                wait.recv().unwrap();
                // Failed delivery drops the guard in its source helper.
                let _ = send.send(guard);
            });
            if mode != "caller_first" {
                crate::lifecycle::owner::test_cancel(&original);
            }
            let first = original.cleanup_end();
            std::thread::sleep(if mode == "caller_end" {
                Duration::from_millis(1100)
            } else {
                Duration::from_millis(60)
            });
            let other_root = tempfile::tempdir().unwrap();
            let other = crate::lifecycle::owner::test_scope(
                other_root.path().to_owned(),
                Duration::from_secs(5),
            );
            crate::lifecycle::owner::test_activate(if mode == "cleared" {
                None
            } else {
                Some(&other)
            });
            let started = std::time::Instant::now();
            if matches!(mode.as_str(), "receiver_drop" | "caller_first") {
                drop(receive);
                release.send(()).unwrap();
            } else {
                release.send(()).unwrap();
                let guard = receive.recv().unwrap();
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let mut child =
                    runtime.block_on(async { async_process::from_guard(guard).unwrap() });
                assert!(
                    runtime.block_on(child.wait()).is_err(),
                    "late output wait ignored original cancellation"
                );
                drop(child);
            }
            helper.join().unwrap();
            assert!(
                started.elapsed() < Duration::from_millis(100),
                "late handoff refreshed cleanup"
            );
            let command_end = cleanup.end();
            let unresolved = root.path().join(pid.to_string()).exists();
            if mode != "caller_first" {
                assert_eq!(original.cleanup_end(), first);
            } else {
                assert!(
                    !original
                        .cancelled
                        .load(std::sync::atomic::Ordering::Acquire)
                );
            }
            assert!(fs::read_dir(other_root.path()).unwrap().next().is_none());
            // The test owns reconciliation after checking the expired receipt.
            let end = std::time::Instant::now() + Duration::from_secs(1);
            while crate::lifecycle::live_group(pid).unwrap() {
                assert!(std::time::Instant::now() < end);
                std::thread::sleep(Duration::from_millis(5));
            }
            // A broken handoff may already have reaped the child; both paths still
            // reconcile this test-owned process before asserting the control.
            let _ = nix::sys::wait::waitpid(nix::unistd::Pid::from_raw(pid as i32), None);
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
            crate::lifecycle::owner::test_activate(None);
            if mode == "caller_first" {
                assert_eq!(
                    command_end, command_first,
                    "caller-first handoff moved its initial owner cap"
                );
            }
            assert!(unresolved, "expired handoff erased unresolved identity");
            return;
        }
        for mode in [
            "cleared",
            "rebound",
            "receiver_drop",
            "caller_end",
            "caller_first",
        ] {
            isolated_control(
                "process::tests::late_async_handoff_keeps_original_cleanup_end_and_registration",
                KEY,
                mode,
            );
        }
    }

    #[test]
    fn created_docker_cleanup_settles_or_retains_only_the_owned_identity() {
        const CHILD: &str = "VEOVEO_TEST_DOCKER_SETTLEMENT";
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        if let Ok(case) = env::var(CHILD) {
            let root = PathBuf::from(env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            let id = "a".repeat(64);
            let result = runtime.block_on(crate::lifecycle::owner::run(async {
                let mut fixture = ContainerGuard::new("preexisting-name-is-not-an-id")?;
                if case != "missing" {
                    fs::write(fixture.cid_file(), &id)?;
                    fixture.record_created(&id)?;
                }
                // The actual registered owner action performs checked cleanup on
                // completion; this intentionally tests that success path too.
                Ok(())
            }));
            if case == "success" {
                assert!(result.is_ok(), "owned cleanup failed: {result:?}");
                assert_eq!(
                    fs::read_to_string(root.join("attempted")).unwrap().trim(),
                    id
                );
                assert!(!root.read_dir().unwrap().any(|entry| {
                    entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".unresolved-owner-")
                }));
            } else {
                assert!(result.is_err(), "unsettled cleanup became success");
                assert!(root.read_dir().unwrap().any(|entry| {
                    entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".unresolved-owner-")
                }));
                let receipt = root
                    .read_dir()
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".unresolved-owner-")
                    })
                    .unwrap();
                let retained: serde_json::Value =
                    serde_json::from_slice(&fs::read(receipt).unwrap()).unwrap();
                assert_eq!(
                    retained[0]["observedIdentity"],
                    if case == "missing" {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::String(id.clone())
                    }
                );
                if case == "missing" {
                    assert!(
                        !root.join("attempted").exists(),
                        "missing creation identity caused name-based deletion"
                    );
                } else {
                    assert_eq!(
                        fs::read_to_string(root.join("attempted")).unwrap().trim(),
                        id
                    );
                }
            }
            return;
        }
        for case in ["success", "refusal", "uncertain", "missing"] {
            let root = tempfile::tempdir().unwrap();
            let bin = root.path().join("bin");
            fs::create_dir(&bin).unwrap();
            let docker = bin.join("docker");
            fs::write(&docker, b"#!/bin/sh\n[ \"$1\" = rm ] && [ \"$2\" = -f ] && [ \"$3\" = --volumes ] || exit 2\nprintf '%s\\n' \"$4\" > \"$VEOVEO_SMOKE_LOCAL_GROUPS/attempted\"\ncase \"$VEOVEO_TEST_DOCKER_SETTLEMENT\" in success) exit 0;; refusal) exit 1;; uncertain) sleep 30;; *) exit 2;; esac\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&docker, fs::Permissions::from_mode(0o700)).unwrap();
            let mut paths = vec![bin];
            paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
            let mut command = Command::new(env::current_exe().unwrap());
            command.args(["--exact", "process::tests::created_docker_cleanup_settles_or_retains_only_the_owned_identity", "--nocapture"])
                .env(CHILD, case)
                .env("PATH", env::join_paths(paths).unwrap())
                .env("VEOVEO_SMOKE_LOCAL_GROUPS", root.path())
                .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "1");
            let mut child = fixture_child(command);
            let end = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "Docker {case} control failed");
                    break;
                }
                assert!(
                    std::time::Instant::now() < end,
                    "Docker {case} control did not finish"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    #[test]
    fn asynchronous_output_admission_deadline_and_cancellation_prevent_late_payload() {
        const MODE: &str = "VEOVEO_TEST_ASYNC_ADMISSION";
        if let Ok(mode) = env::var(MODE) {
            let root = PathBuf::from(env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime
                .block_on(crate::lifecycle::owner::run(async {
                    let marker = root.join(".payload");
                    let mut command = tokio::process::Command::new("/bin/sh");
                    command
                        .args(["-c", "printf forbidden > \"$MARKER\""])
                        .env("MARKER", &marker);
                    // Model slow admission before the maintained gate's release hook.
                    // SAFETY: nanosleep is async-signal-safe; the hook allocates nothing.
                    unsafe {
                        command.pre_exec(|| {
                            let delay = nix::libc::timespec {
                                tv_sec: 0,
                                tv_nsec: 300_000_000,
                            };
                            nix::libc::nanosleep(&delay, std::ptr::null_mut());
                            Ok(())
                        });
                    }
                    let ticks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
                    let polling = std::sync::Arc::clone(&ticks);
                    let ticker = tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                            polling.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    });
                    let started = std::time::Instant::now();
                    let result = if mode == "deadline" {
                        output_async(command, Duration::from_millis(50)).await
                    } else {
                        tokio::time::timeout(
                            Duration::from_millis(50),
                            output_async(command, Duration::from_secs(10)),
                        )
                        .await
                        .map_err(anyhow::Error::from)
                        .and_then(|result| result)
                    };
                    ensure!(
                        result.is_err(),
                        "slow admission passed caller deadline/cancellation"
                    );
                    ensure!(
                        started.elapsed() < Duration::from_millis(200),
                        "admission blocked Tokio or exceeded caller deadline"
                    );
                    ensure!(
                        ticks.load(std::sync::atomic::Ordering::Relaxed) >= 3,
                        "Tokio polling stopped during admission"
                    );
                    let end = std::time::Instant::now() + Duration::from_secs(2);
                    loop {
                        let entries = fs::read_dir(&root)?.collect::<std::io::Result<Vec<_>>>()?;
                        if !entries.iter().any(|entry| {
                            entry.file_name().to_string_lossy().starts_with(".launch-")
                        }) {
                            break;
                        }
                        ensure!(
                            std::time::Instant::now() < end,
                            "admission receipt did not settle within one-second cleanup"
                        );
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    tokio::time::sleep(Duration::from_millis(350)).await;
                    ensure!(
                        !marker.exists(),
                        "late admission executed payload after caller stopped"
                    );
                    ensure!(
                        !fs::read_dir(&root)?.any(|entry| entry
                            .unwrap()
                            .file_name()
                            .to_string_lossy()
                            .parse::<u32>()
                            .is_ok()),
                        "settled admission left group registration"
                    );
                    ticker.abort();
                    let _ = ticker.await;
                    Ok(())
                }))
                .unwrap();
            return;
        }
        for mode in ["deadline", "cancel"] {
            let root = tempfile::tempdir().unwrap();
            let mut command = Command::new(env::current_exe().unwrap());
            command.args(["--exact", "process::tests::asynchronous_output_admission_deadline_and_cancellation_prevent_late_payload", "--nocapture"])
                .env(MODE, mode).env("VEOVEO_SMOKE_LOCAL_GROUPS", root.path());
            let mut child = fixture_child(command);
            let end = std::time::Instant::now() + Duration::from_secs(4);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(
                        status.success(),
                        "slow async admission {mode} control failed"
                    );
                    break;
                }
                assert!(
                    std::time::Instant::now() < end,
                    "slow admission control did not finish"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    #[test]
    fn asynchronous_output_timeout_and_cancellation_kill_ignoring_group() {
        const MODE: &str = "VEOVEO_TEST_ASYNC_ABORT";
        if let Ok(mode) = env::var(MODE) {
            let root = PathBuf::from(env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(crate::lifecycle::owner::run(async {
                let mut command = tokio::process::Command::new("sh");
                command.args(["-c", r#"trap '' INT TERM; echo $$ > "$1/.leader"; sleep 60 & echo $! > "$1/.descendant"; wait"#, "owned-control"])
                    .arg(&root);
                let start = std::time::Instant::now();
                let result = if mode == "timeout" {
                    output_async(command, Duration::from_millis(150)).await
                } else {
                    tokio::time::timeout(Duration::from_millis(150),
                        output_async(command, Duration::from_secs(60))).await
                        .map_err(anyhow::Error::from).and_then(|result| result)
                };
                ensure!(result.is_err(), "stalled owned process passed");
                ensure!(start.elapsed() < Duration::from_secs(2), "command used wide outer owner budget");
                let leader = fs::read_to_string(root.join(".leader"))?;
                let pid: u32 = leader.trim().parse()?;
                ensure!(!Path::new(&format!("/proc/{pid}")).exists(), "owned leader was not reaped");
                ensure!(!crate::lifecycle::live_group(pid)?, "owned descendants survived cancellation");
                ensure!(!root.join(leader.trim()).exists(), "settled process registration survived reaping");
                Ok(())
            })).unwrap();
            return;
        }
        for mode in ["timeout", "cancel"] {
            let root = tempfile::tempdir().unwrap();
            let mut command = Command::new(env::current_exe().unwrap());
            command.args(["--exact", "process::tests::asynchronous_output_timeout_and_cancellation_kill_ignoring_group", "--nocapture"])
                .env(MODE, mode).env("VEOVEO_SMOKE_LOCAL_GROUPS", root.path())
                .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "10");
            let mut child = fixture_child(command);
            let end = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "async {mode} abort control failed");
                    break;
                }
                assert!(
                    std::time::Instant::now() < end,
                    "async abort control exceeded deadline"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    #[test]
    fn asynchronous_output_preserves_caller_budget_and_owner_deadline() {
        const MODE: &str = "VEOVEO_TEST_OUTPUT_BUDGET";
        if let Ok(mode) = env::var(MODE) {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = runtime.block_on(crate::lifecycle::owner::run(async {
                let mut command = tokio::process::Command::new("sh");
                command.args(["-c", "sleep 0.2; printf caller-output"]);
                let output = output_async(command, Duration::from_secs(2)).await?;
                ensure!(output.status.success(), "owned caller failed");
                ensure!(output.stdout == b"caller-output", "caller stdout changed");
                Ok(())
            }));
            if mode == "caller" {
                assert!(result.is_ok(), "caller budget was narrowed: {result:?}");
            } else {
                assert!(result.is_err(), "caller budget extended owner deadline");
            }
            return;
        }
        for mode in ["caller", "owner"] {
            let root = tempfile::tempdir().unwrap();
            let end = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                + if mode == "caller" {
                    Duration::from_secs(5)
                } else {
                    Duration::from_millis(100)
                };
            let mut command = Command::new(env::current_exe().unwrap());
            command.args(["--exact", "process::tests::asynchronous_output_preserves_caller_budget_and_owner_deadline", "--nocapture"])
                .env(MODE, mode)
                .env("VEOVEO_SMOKE_LOCAL_GROUPS", root.path())
                .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", end.as_millis().to_string())
                .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "1");
            let mut child = fixture_child(command);
            let end = std::time::Instant::now() + Duration::from_secs(6);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "output budget {mode} control failed");
                    break;
                }
                assert!(
                    std::time::Instant::now() < end,
                    "output budget control exceeded deadline"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
