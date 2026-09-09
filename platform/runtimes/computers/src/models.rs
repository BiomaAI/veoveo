use crate::{
    Binding, canonical,
    protocol::{sandbox::v1::SandboxPolicy, v1 as api},
    storage::PersistentHome,
};
pub use api::SandboxPhase as Phase;
use sha2::{Digest, Sha256};
use uuid::Uuid;
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;
pub type Result<T> = std::result::Result<T, RuntimeFailure>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RuntimeFailure {
    #[error("invalid installation runtime configuration")]
    InvalidConfiguration,
    #[error("development template is outside the admitted profile")]
    InvalidTemplate,
    #[error("runtime binding mismatch")]
    BindingMismatch,
    #[error("OpenShell unavailable or authentication failed")]
    Unavailable,
    #[error("OpenShell gateway version or driver differs from the admitted runtime")]
    VersionMismatch,
    #[error("computer runtime was not found")]
    NotFound,
    #[error("computer is not in the required lifecycle state; files are retained")]
    InvalidState,
    #[error("OpenShell lifecycle operation failed; outcome is unknown and files are retained")]
    LifecycleUnknown,
    #[error("OpenShell lifecycle watch failed or lost continuity; files are retained")]
    WatchFailed,
    #[error("computer storage preparation failed; existing files are retained")]
    AllocationFailed,
    #[error(
        "replacement policy continuity is not confirmed; retained runtime state must be reconciled"
    )]
    PolicyContinuity,
    #[error("terminal authorization or canonical attachment failed")]
    TerminalFailed,
    #[error("terminal connection lease expired")]
    LeaseExpired,
    #[error("terminal frame or dimensions exceed their bounds")]
    TerminalBounds,
    #[error("execution intent is outside the admitted bounds")]
    InvalidExecution,
    #[error("execution could not be established; outcome is unknown")]
    ExecutionUnknown,
}

pub(crate) fn valid_fingerprint(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn canonical_path(p: &str) -> bool {
    p.starts_with('/')
        && !p.contains('\0')
        && (p == "/"
            || p[1..]
                .split('/')
                .all(|c| !c.is_empty() && c != "." && c != ".."))
}
fn overlaps(a: &str, b: &str) -> bool {
    a == b
        || a == "/"
        || b.strip_prefix(a).is_some_and(|p| p.starts_with('/'))
        || a.strip_prefix(b).is_some_and(|p| p.starts_with('/'))
}

#[derive(Clone)]
pub struct DevelopmentTemplate {
    image: String,
    cpus: u32,
    memory_mib: u32,
    policy: SandboxPolicy,
    command: Vec<String>,
    persistent_home: Option<PersistentHome>,
}
impl DevelopmentTemplate {
    pub fn new(
        image: String,
        cpus: u32,
        memory_mib: u32,
        policy: SandboxPolicy,
        command: Vec<String>,
        persistent_home: Option<PersistentHome>,
    ) -> Result<Self> {
        let fail = RuntimeFailure::InvalidTemplate;
        let (repo, hash) = image.split_once("@sha256:").ok_or(fail)?;
        if !repo.starts_with(|c: char| c.is_ascii_alphanumeric())
            || !repo
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"./:_-".contains(&c))
            || !valid_fingerprint(hash)
            || !(1..=64).contains(&cpus)
            || !(512..=262144).contains(&memory_mib)
            || command.is_empty()
            || command.len() > 16
            || !command[0].starts_with('/')
            || command
                .iter()
                .any(|s| s.is_empty() || s.chars().count() > 512 || s.contains('\0'))
            || policy.version != 1
            || policy
                .landlock
                .as_ref()
                .is_none_or(|p| p.compatibility != "hard_requirement")
        {
            return Err(fail);
        }
        let fs = policy.filesystem.as_ref().ok_or(fail)?;
        let identity = policy.process.as_ref().ok_or(fail)?;
        for id in [&identity.run_as_user, &identity.run_as_group] {
            if id.is_empty()
                || id.len() > 9
                || id.starts_with('0')
                || !id.bytes().all(|c| c.is_ascii_digit())
            {
                return Err(fail);
            }
        }
        if let Some(home) = &persistent_home {
            home.validate(&policy, &command, memory_mib)?;
        } else if !fs.include_workdir {
            return Err(fail);
        }
        for path in fs.read_only.iter().chain(&fs.read_write) {
            if !canonical_path(path) {
                return Err(fail);
            }
            if [
                "/etc/openshell-tls/ca-bundle.pem",
                "/etc/openshell-tls/openshell-ca.pem",
            ]
            .contains(&path.as_str())
                && !fs.read_write.contains(path)
            {
                continue;
            }
            if [
                "/opt/openshell",
                "/etc/openshell",
                "/etc/openshell-tls",
                "/run/openshell",
                "/run/openshell-sidecar",
                "/var/run/docker.sock",
            ]
            .iter()
            .any(|c| overlaps(path, c))
            {
                return Err(fail);
            }
        }
        for rule in policy.network_policies.values() {
            if rule.endpoints.is_empty() || rule.binaries.is_empty() {
                return Err(fail);
            }
            for e in &rule.endpoints {
                if matches!(e.protocol.to_ascii_lowercase().as_str(), "" | "tcp") {
                    if !e.enforcement.is_empty() {
                        return Err(fail);
                    }
                } else if e.enforcement != "enforce" {
                    return Err(fail);
                }
            }
        }
        if policy
            .network_middlewares
            .values()
            .any(|m| m.on_error != "fail_closed")
        {
            return Err(fail);
        }
        Ok(Self {
            image,
            cpus,
            memory_mib,
            policy,
            command,
            persistent_home,
        })
    }
    pub(crate) fn admits_network_policy(&self, effective: &SandboxPolicy) -> bool {
        let mut policy = self.policy.clone();
        policy
            .network_policies
            .clone_from(&effective.network_policies);
        Self::new(
            self.image.clone(),
            self.cpus,
            self.memory_mib,
            policy,
            self.command.clone(),
            self.persistent_home.clone(),
        )
        .is_ok()
    }
    fn base_spec(&self) -> api::SandboxSpec {
        api::SandboxSpec {
            log_level: "warn".into(),
            template: Some(api::SandboxTemplate {
                image: self.image.clone(),
                resources: Some(crate::storage::object([(
                    "limits",
                    crate::storage::struct_value(crate::storage::object([
                        ("cpu", crate::storage::string_value(self.cpus.to_string())),
                        (
                            "memory",
                            crate::storage::string_value(format!("{}Mi", self.memory_mib)),
                        ),
                    ])),
                )])),
                ..Default::default()
            }),
            policy: Some(self.policy.clone()),
            command: self.command.clone(),
            tty: true,
            ..Default::default()
        }
    }
    pub fn spec(&self, id: Uuid) -> Result<api::SandboxSpec> {
        let mut spec = self.base_spec();
        if let Some(home) = &self.persistent_home {
            spec.template.as_mut().unwrap().driver_config = Some(home.driver_config(id)?);
        }
        Ok(spec)
    }
    pub fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(canonical::encode(
            &self.base_spec(),
            ".openshell.v1.SandboxSpec",
        ));
        if let Some(home) = &self.persistent_home {
            hash.update(b"\0veoveo-persistent-home\0");
            hash.update(home.fingerprint_bytes());
        }
        hex::encode(hash.finalize())
    }
    pub fn persistent_home(&self) -> Option<&PersistentHome> {
        self.persistent_home.as_ref()
    }
}

#[derive(Clone, PartialEq)]
pub struct Observation {
    pub sandbox_id: String,
    pub phase: Phase,
    pub main_process_instance_id: String,
    pub exit_code: Option<i32>,
}
impl Observation {
    pub(crate) fn checked(
        sandbox: api::Sandbox,
        binding: &Binding,
        workspace: &str,
    ) -> Result<Self> {
        let m = sandbox.metadata.ok_or(RuntimeFailure::BindingMismatch)?;
        let s = sandbox.status.ok_or(RuntimeFailure::BindingMismatch)?;
        if m.name != binding.name()
            || m.workspace != workspace
            || !binding.labels_match(&m.labels)
            || !identifier(&m.id)
            || (!s.main_process_instance_id.is_empty() && !identifier(&s.main_process_instance_id))
        {
            return Err(RuntimeFailure::BindingMismatch);
        }
        Ok(Self {
            sandbox_id: m.id,
            phase: Phase::try_from(s.phase).map_err(|_| RuntimeFailure::BindingMismatch)?,
            main_process_instance_id: s.main_process_instance_id,
            exit_code: s.exit_code,
        })
    }
    pub(crate) fn same_process(&self, other: &Self) -> bool {
        other.phase == Phase::Ready
            && self.sandbox_id == other.sandbox_id
            && self.main_process_instance_id == other.main_process_instance_id
    }
}
pub(crate) fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    pub(crate) cols: u32,
    pub(crate) rows: u32,
}
impl TerminalSize {
    pub fn new(cols: u32, rows: u32) -> Result<Self> {
        if !(2..=500).contains(&cols) || !(1..=200).contains(&rows) {
            return Err(RuntimeFailure::TerminalBounds);
        }
        Ok(Self { cols, rows })
    }
    pub fn cols(self) -> u32 {
        self.cols
    }
    pub fn rows(self) -> u32 {
        self.rows
    }
}
