//! File-backed output avoids pipe backpressure and descendant-held pipe waits.
use anyhow::{Context, Result, ensure};
use nix::{
    sys::signal::{Signal, killpg},
    sys::wait::{Id, WaitPidFlag, WaitStatus, waitid},
    unistd::Pid,
};
use std::os::unix::process::CommandExt;
use std::{
    io::{Read, Seek, SeekFrom},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

struct OwnedChild {
    child: Child,
    settled: bool,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let _ = killpg(Pid::from_raw(self.child.id() as i32), Signal::SIGKILL);
        let _ = self.child.kill();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

fn captured(command: &mut Command, seconds: u64) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command.process_group(0);
    let child = command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()
        .context("start fixture command")?;
    let mut owned = OwnedChild {
        child,
        settled: false,
    };
    let started = Instant::now();
    let status = loop {
        // Observe without reaping: the child's PID keeps the owned group identity
        // reserved until descendants are killed, avoiding a PID-reuse signal race.
        let pid = Pid::from_raw(owned.child.id() as i32);
        match waitid(
            Id::Pid(pid),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        ) {
            Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => {
                let _ = killpg(pid, Signal::SIGKILL);
                let status = owned
                    .child
                    .try_wait()?
                    .context("observed child settlement was lost")?;
                owned.settled = true;
                break status;
            }
            Ok(WaitStatus::StillAlive) if started.elapsed() < Duration::from_secs(seconds) => {
                thread::sleep(Duration::from_millis(30))
            }
            Ok(WaitStatus::StillAlive) => {
                anyhow::bail!("fixture command exceeded {seconds} seconds")
            }
            _ => anyhow::bail!("fixture command observation failed"),
        }
        ensure!(
            stdout.metadata()?.len() <= 2 * 1024 * 1024
                && stderr.metadata()?.len() <= 2 * 1024 * 1024,
            "fixture command output exceeded 2 MiB"
        );
    };
    ensure!(
        stdout.metadata()?.len() <= 2 * 1024 * 1024,
        "fixture output exceeded 2 MiB"
    );
    stdout.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    stdout.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 2 * 1024 * 1024,
        "fixture output exceeded 2 MiB"
    );
    ensure!(
        stderr.metadata()?.len() <= 2 * 1024 * 1024,
        "fixture stderr exceeded 2 MiB"
    );
    stderr.seek(SeekFrom::Start(0))?;
    let mut diagnostics = Vec::new();
    stderr
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut diagnostics)?;
    ensure!(
        diagnostics.len() <= 2 * 1024 * 1024,
        "fixture stderr exceeded 2 MiB"
    );
    Ok((status, bytes, diagnostics))
}
#[cfg(test)]
fn output(command: &mut Command, seconds: u64) -> Result<(bool, Vec<u8>)> {
    let (status, bytes, _) = captured(command, seconds)?;
    Ok((status.success(), bytes))
}
pub(super) fn checked(command: &mut Command, seconds: u64) -> Result<Vec<u8>> {
    checked_phase(command, seconds, "fixture command")
}
fn program(command: &Command) -> &str {
    match command.get_program().to_str() {
        Some("helm") => "helm",
        Some("kubectl") => "kubectl",
        Some("openssl") => "openssl",
        _ => "fixture subprocess",
    }
}
pub(super) fn checked_phase(command: &mut Command, seconds: u64, phase: &str) -> Result<Vec<u8>> {
    checked_with_diagnostics(command, seconds, phase, |_| {
        "stderr excluded to protect credentials".into()
    })
}
/// Diagnostic identity only; it never establishes an operation outcome.
#[derive(Clone, Copy, Debug)]
pub(super) enum KubernetesOperation {
    Apply,
    Create,
    Get,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum KubernetesPurpose {
    FixtureObject,
    ManagedInstallation,
    InstallationJobObservation,
}
#[derive(Clone, Copy, Debug)]
enum KubernetesResourceKind {
    Namespace,
    Secret,
    ConfigMap,
    ServiceAccount,
    Role,
    RoleBinding,
    ValidatingAdmissionPolicy,
    ValidatingAdmissionPolicyBinding,
    NetworkPolicy,
    Service,
    Deployment,
    StatefulSet,
    Job,
    Pod,
}
#[derive(Debug)]
pub(super) struct KubernetesResource {
    kind: KubernetesResourceKind,
    namespace: Option<String>,
    name: String,
}
impl KubernetesResource {
    pub(super) fn new(kind: &str, namespace: Option<&str>, name: &str) -> Result<Self> {
        let kind = match kind {
            "Namespace" => KubernetesResourceKind::Namespace,
            "Secret" => KubernetesResourceKind::Secret,
            "ConfigMap" => KubernetesResourceKind::ConfigMap,
            "ServiceAccount" => KubernetesResourceKind::ServiceAccount,
            "Role" => KubernetesResourceKind::Role,
            "RoleBinding" => KubernetesResourceKind::RoleBinding,
            "ValidatingAdmissionPolicy" => KubernetesResourceKind::ValidatingAdmissionPolicy,
            "ValidatingAdmissionPolicyBinding" => {
                KubernetesResourceKind::ValidatingAdmissionPolicyBinding
            }
            "NetworkPolicy" => KubernetesResourceKind::NetworkPolicy,
            "Service" => KubernetesResourceKind::Service,
            "Deployment" => KubernetesResourceKind::Deployment,
            "StatefulSet" => KubernetesResourceKind::StatefulSet,
            "Job" => KubernetesResourceKind::Job,
            "Pod" => KubernetesResourceKind::Pod,
            _ => anyhow::bail!("unsupported diagnostic resource kind"),
        };
        fn admitted(value: &str, limit: usize, dots: bool) -> bool {
            !value.is_empty()
                && value.len() <= limit
                && value.split('.').all(|part| {
                    !part.is_empty()
                        && part.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                        && part.ends_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                        && part
                            .bytes()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                })
                && (dots || !value.contains('.'))
        }
        ensure!(
            admitted(name, 253, true),
            "invalid diagnostic resource name"
        );
        ensure!(
            namespace.is_none_or(|ns| admitted(ns, 63, false)),
            "invalid diagnostic resource namespace"
        );
        Ok(Self {
            kind,
            namespace: namespace.map(str::to_owned),
            name: name.into(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KubernetesFailureCategory {
    Invalid,
    Forbidden,
    AlreadyExists,
    NotFound,
    Conflict,
    Unauthorized,
    BadRequest,
    Unclassified,
}
fn kubernetes_failure_category(stderr: &[u8]) -> KubernetesFailureCategory {
    let Ok(text) = std::str::from_utf8(stderr) else {
        return KubernetesFailureCategory::Unclassified;
    };
    [
        ("Invalid", KubernetesFailureCategory::Invalid),
        ("Forbidden", KubernetesFailureCategory::Forbidden),
        ("AlreadyExists", KubernetesFailureCategory::AlreadyExists),
        ("NotFound", KubernetesFailureCategory::NotFound),
        ("Conflict", KubernetesFailureCategory::Conflict),
        ("Unauthorized", KubernetesFailureCategory::Unauthorized),
        ("BadRequest", KubernetesFailureCategory::BadRequest),
    ]
    .into_iter()
    .find_map(|(reason, category)| {
        text.starts_with(&format!("Error from server ({reason}):"))
            .then_some(category)
    })
    .unwrap_or(KubernetesFailureCategory::Unclassified)
}
pub(super) fn checked_kubernetes(
    command: &mut Command,
    seconds: u64,
    purpose: KubernetesPurpose,
    operation: KubernetesOperation,
    resource: &KubernetesResource,
) -> Result<Vec<u8>> {
    ensure!(
        program(command) == "kubectl",
        "Kubernetes diagnostics require kubectl"
    );
    kubernetes_command(command, seconds, purpose, operation, resource)
}
fn kubernetes_command(
    command: &mut Command,
    seconds: u64,
    purpose: KubernetesPurpose,
    operation: KubernetesOperation,
    resource: &KubernetesResource,
) -> Result<Vec<u8>> {
    let phase = format!(
        "{purpose:?}: {operation:?} {:?} {}/{}",
        resource.kind,
        resource.namespace.as_deref().unwrap_or("cluster"),
        resource.name
    );
    checked_with_diagnostics(command, seconds, &phase, |stderr| {
        format!(
            "Kubernetes category {:?}; stderr excluded to protect credentials",
            kubernetes_failure_category(stderr)
        )
    })
}
/// Only the fixture-owned Helm values boundary opts into redacted stderr.
pub(super) fn checked_redacted(
    command: &mut Command,
    seconds: u64,
    phase: &str,
    redact: impl FnOnce(&[u8]) -> String,
) -> Result<Vec<u8>> {
    ensure!(
        program(command) == "helm",
        "redacted command diagnostics require the fixture Helm boundary"
    );
    checked_with_diagnostics(command, seconds, phase, redact)
}
fn checked_with_diagnostics(
    command: &mut Command,
    seconds: u64,
    phase: &str,
    redact: impl FnOnce(&[u8]) -> String,
) -> Result<Vec<u8>> {
    let label = program(command).to_owned();
    let (status, bytes, stderr) =
        captured(command, seconds).with_context(|| format!("{phase}: {label}"))?;
    if !status.success() {
        anyhow::bail!("{phase}: {label} failed ({status}); {}", redact(&stderr));
    }
    Ok(bytes)
}

/// A fixture-owned watch or port-forward. Output is file-backed and never logged.
pub(super) struct Background {
    owned: OwnedChild,
    stdout: std::fs::File,
    stderr: std::fs::File,
    started: Instant,
    seconds: u64,
    offset: u64,
    observer: &'static str,
}
impl Background {
    pub fn start(command: &mut Command, seconds: u64, observer: &'static str) -> Result<Self> {
        let stdout = tempfile::tempfile()?;
        let stderr = tempfile::tempfile()?;
        command.process_group(0);
        let child = command
            .stdin(Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?)
            .spawn()
            .context("start owned fixture observer")?;
        Ok(Self {
            owned: OwnedChild {
                child,
                settled: false,
            },
            stdout,
            stderr,
            started: Instant::now(),
            seconds,
            offset: 0,
            observer,
        })
    }
    pub fn read(&mut self) -> Result<Vec<u8>> {
        ensure!(
            self.started.elapsed() < Duration::from_secs(self.seconds),
            "fixture observer deadline exceeded"
        );
        ensure!(
            self.stdout.metadata()?.len() <= 2 * 1024 * 1024
                && self.stderr.metadata()?.len() <= 2 * 1024 * 1024,
            "fixture observer output exceeded 2 MiB"
        );
        let status = waitid(
            Id::Pid(Pid::from_raw(self.owned.child.id() as i32)),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        )?;
        match status {
            WaitStatus::StillAlive => (),
            WaitStatus::Exited(_, code) => anyhow::bail!(
                "{} ended (exit status: {code}); an observation gap cannot prove drain",
                self.observer
            ),
            WaitStatus::Signaled(_, signal, _) => anyhow::bail!(
                "{} ended (signal: {signal}); an observation gap cannot prove drain",
                self.observer
            ),
            _ => anyhow::bail!(
                "{} returned an unexpected child state; an observation gap cannot prove drain",
                self.observer
            ),
        }
        // pread keeps the child's shared open-file write offset untouched.
        use std::os::unix::fs::FileExt;
        let mut bytes = vec![0; (self.stdout.metadata()?.len() - self.offset) as usize];
        self.stdout.read_exact_at(&mut bytes, self.offset)?;
        self.offset += bytes.len() as u64;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kubernetes_failure_preserves_resource_and_status_without_child_content() -> Result<()> {
        let resource =
            KubernetesResource::new("Secret", Some("fixture-agent"), "runtime-credentials")?;
        for reason in ["Invalid", "Forbidden"] {
            let payload = format!(
                "Error from server ({reason}): stringData.password=secret-sentinel SQL token header body"
            );
            let error = kubernetes_command(
                Command::new("sh").args([
                    "-c",
                    "printf '%s' \"$1\" >&2; exit 17",
                    "private-argv-sentinel",
                    &payload,
                ]),
                2,
                KubernetesPurpose::ManagedInstallation,
                KubernetesOperation::Apply,
                &resource,
            )
            .unwrap_err();
            let diagnostic = format!("{error:#}");
            assert!(
                diagnostic.contains(
                    "ManagedInstallation: Apply Secret fixture-agent/runtime-credentials"
                )
            );
            assert!(diagnostic.contains("exit status: 17"));
            assert!(diagnostic.contains(&format!("Kubernetes category {reason}")));
            for private in [
                "secret-sentinel",
                "private-argv-sentinel",
                "stringData",
                "SQL",
                "header",
                "printf",
            ] {
                assert!(!diagnostic.contains(private));
            }
        }
        for payload in [
            "prefix Error from server (Forbidden): secret-sentinel",
            "Error from server (Other): secret-sentinel",
            "Error from server (Forbidden) secret-sentinel",
            "secret-sentinel\nError from server (Invalid): leak",
        ] {
            let error = kubernetes_command(
                Command::new("sh").args([
                    "-c",
                    "printf '%s' \"$1\" >&2; exit 3",
                    "fixture",
                    payload,
                ]),
                2,
                KubernetesPurpose::ManagedInstallation,
                KubernetesOperation::Create,
                &resource,
            )
            .unwrap_err();
            let diagnostic = format!("{error:#}");
            assert!(diagnostic.contains("Kubernetes category Unclassified"));
            assert!(!diagnostic.contains("secret-sentinel"));
        }
        assert_eq!(
            kubernetes_failure_category(&[0xff]),
            KubernetesFailureCategory::Unclassified
        );
        Ok(())
    }
    #[test]
    fn kubernetes_identity_refuses_unsafe_fields_without_echoing_them() {
        for (kind, namespace, name) in [
            ("Secret secret-sentinel", Some("fixture"), "safe"),
            ("Secret", Some("fixture\nsecret-sentinel"), "safe"),
            ("Secret", Some("fixture"), "safe/secret-sentinel"),
        ] {
            let error = KubernetesResource::new(kind, namespace, name).unwrap_err();
            assert!(!format!("{error:#}").contains("secret-sentinel"));
        }
    }
    #[test]
    fn generic_command_diagnostics_suppress_child_secrets() {
        let error = checked_phase(
            Command::new("sh").args(["-c", "echo secret=password-123 >&2; exit 7"]),
            2,
            "native diagnostic",
        )
        .unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("native diagnostic"));
        assert!(diagnostic.contains("exit status: 7"));
        assert!(!diagnostic.contains("password-123"));
    }
    #[test]
    fn background_exit_diagnostics_name_observer_and_status_without_child_secrets() {
        let mut background = Background::start(
            Command::new("sh").args(["-c", "echo secret=password-123 >&2; exit 7"]),
            2,
            "managed Pod watch",
        )
        .unwrap();
        let started = Instant::now();
        let diagnostic = loop {
            match background.read() {
                Err(error) => break format!("{error:#}"),
                Ok(_) => {
                    assert!(started.elapsed() < Duration::from_secs(1));
                    thread::sleep(Duration::from_millis(10));
                }
            }
        };
        assert!(diagnostic.contains("managed Pod watch ended (exit status: 7)"));
        assert!(!diagnostic.contains("password-123"));
        assert!(!diagnostic.contains("echo secret"));
    }
    #[test]
    fn command_deadline_and_failure_are_distinct() {
        assert!(!output(Command::new("false").arg(""), 1).unwrap().0);
        let start = Instant::now();
        assert!(output(Command::new("sleep").arg("5"), 1).is_err());
        assert!(start.elapsed() < Duration::from_secs(4));
    }
    #[test]
    fn deadline_kills_owned_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("descendant.pid");
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30 & echo $! > \"$1\"; wait", "fixture"])
            .arg(&path);
        assert!(output(&mut command, 1).is_err());
        let pid = std::fs::read_to_string(&path).unwrap().trim().to_owned();
        assert_stopped(&pid);
    }

    #[test]
    fn parent_success_retires_background_descendants_before_output() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("descendant.pid");
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30 & echo $! > \"$1\"; exit 0", "fixture"])
            .arg(&path);
        assert!(output(&mut command, 1).unwrap().0);
        let pid = std::fs::read_to_string(path).unwrap().trim().to_owned();
        assert_stopped(&pid);
    }
    fn assert_stopped(pid: &str) {
        let start = Instant::now();
        loop {
            let Ok(state) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return;
            };
            if state.split_whitespace().nth(2) == Some("Z") {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "owned descendant still executes after group kill"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
