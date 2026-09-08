//! Cluster-wide serialization for cooperating disposable profile installers.
//!
//! This lock has no expiry or automatic takeover. It remains after an interrupted
//! operation because a detached Helm/kubectl child may still be writing resources.
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use veoveo_deploy_contract::components::{
    InstallationCoordination, LockedComponent, ObjectIdentity,
};

const NAMESPACE: &str = "kube-system";
const NAME: &str = "veoveo-profile-mutation";
const API_VERSION: &str = "coordination.k8s.io/v1";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Lease {
    api_version: String,
    kind: String,
    metadata: Metadata,
    spec: Spec,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    namespace: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource_version: Option<String>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    holder_identity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lease_duration_seconds: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteOptions<'a> {
    api_version: &'static str,
    kind: &'static str,
    preconditions: Preconditions<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Preconditions<'a> {
    uid: &'a str,
    resource_version: &'a str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Executing,
    Released,
}

pub(super) struct ExecutionLock {
    context: String,
    lease: Lease,
    phase: Phase,
}

impl ExecutionLock {
    pub(super) fn acquire(context: &str) -> Result<Self> {
        Self::at(context, NAMESPACE, NAME)
    }

    fn at(context: &str, namespace: &str, name: &str) -> Result<Self> {
        // The random temporary name is only an operation identifier, never a
        // credential or a lock. The Kubernetes create supplies exclusivity.
        let nonce = tempfile::Builder::new().prefix("veoveo-").tempdir()?;
        let holder = format!(
            "{}-{}",
            nonce
                .path()
                .file_name()
                .context("operation has no nonce")?
                .to_string_lossy(),
            std::process::id()
        );
        let desired = Lease {
            api_version: API_VERSION.into(),
            kind: "Lease".into(),
            metadata: Metadata {
                namespace: namespace.into(),
                name: name.into(),
                uid: None,
                resource_version: None,
                labels: BTreeMap::from([(
                    "app.kubernetes.io/managed-by".into(),
                    "veoveo-profile".into(),
                )]),
            },
            spec: Spec {
                holder_identity: holder,
                lease_duration_seconds: None,
            },
        };
        // An occupied lock is rejected before attempting a write. A concurrent
        // create after this read is still rejected atomically by the API server.
        let existing = Command::new("kubectl")
            .args([
                "--context",
                context,
                "--request-timeout=10s",
                "get",
                "lease",
                name,
                "--namespace",
                namespace,
                "--ignore-not-found",
                "--output=json",
            ])
            .output()
            .context("reading profile execution lock")?;
        ensure!(
            existing.status.success(),
            "cannot read profile execution lock: {}",
            String::from_utf8_lossy(&existing.stderr)
        );
        ensure!(
            existing.stdout.is_empty(),
            "another operation holds profile execution lock {namespace}/{name}; stop its installer and children before recovering the lock"
        );
        let bytes = request(context, &["create", "--filename=-", "--output=json"], &desired)
            .context("acquiring profile execution lock; an uncertain create must be inspected before recovery")?;
        let lease: Lease =
            serde_json::from_slice(&bytes).context("decoding acquired profile execution lock")?;
        ensure!(
            lease.api_version == API_VERSION
                && lease.kind == "Lease"
                && lease.metadata.namespace == namespace
                && lease.metadata.name == name
                && lease.spec.holder_identity == desired.spec.holder_identity
                && lease.spec.lease_duration_seconds.is_none(),
            "API returned a different profile execution lock"
        );
        identity(&lease)?;
        Ok(Self {
            context: context.into(),
            lease,
            phase: Phase::Preparing,
        })
    }

    pub(super) fn begin_execution(&mut self) -> Result<()> {
        self.check()?;
        self.phase = Phase::Executing;
        Ok(())
    }

    pub(super) fn check(&self) -> Result<()> {
        ensure!(
            self.phase != Phase::Released,
            "profile execution lock was already released"
        );
        let current = Command::new("kubectl")
            .args([
                "--context",
                &self.context,
                "--request-timeout=10s",
                "get",
                "lease",
                &self.lease.metadata.name,
                "--namespace",
                &self.lease.metadata.namespace,
                "--output=json",
            ])
            .output()
            .context("checking profile execution lock")?;
        ensure!(
            current.status.success(),
            "profile execution lock is unavailable: {}",
            String::from_utf8_lossy(&current.stderr)
        );
        validate_current(&self.lease, &serde_json::from_slice(&current.stdout)?)
    }

    pub(super) fn release(&mut self) -> Result<InstallationCoordination> {
        self.check()?;
        let (uid, resource_version) = identity(&self.lease)?;
        let path = format!(
            "/apis/coordination.k8s.io/v1/namespaces/{}/leases/{}",
            self.lease.metadata.namespace, self.lease.metadata.name
        );
        request(
            &self.context,
            &["delete", "--raw", &path, "--filename=-"],
            &DeleteOptions {
                api_version: "v1",
                kind: "DeleteOptions",
                preconditions: Preconditions {
                    uid,
                    resource_version,
                },
            },
        )
        .context("releasing profile execution lock with exact UID and resource version")?;
        self.phase = Phase::Released;
        Ok(InstallationCoordination {
            object: ObjectIdentity {
                group: "coordination.k8s.io".into(),
                kind: "Lease".into(),
                namespace: Some(self.lease.metadata.namespace.clone()),
                name: self.lease.metadata.name.clone(),
            },
            uid: uid.into(),
            holder_identity: self.lease.spec.holder_identity.clone(),
            released: true,
        })
    }
}

impl Drop for ExecutionLock {
    fn drop(&mut self) {
        // Read-only replanning has no detached mutation command to retain.
        if self.phase == Phase::Preparing && self.release().is_ok() {
            return;
        }
        if self.phase != Phase::Released {
            eprintln!(
                "Profile execution lock {}/{} is retained after incomplete execution. Stop the installer and its Helm/kubectl children before recovering UID {}.",
                self.lease.metadata.namespace,
                self.lease.metadata.name,
                self.lease.metadata.uid.as_deref().unwrap_or("unknown")
            );
        }
    }
}

pub(super) fn validate_reserved_identity(catalog: &[LockedComponent]) -> Result<()> {
    ensure!(
        !catalog.iter().any(
            |component| component
                .declaration
                .permitted_objects
                .iter()
                .any(|object| object.group == "coordination.k8s.io"
                    && object.kind == "Lease"
                    && object.namespace.as_deref() == Some(NAMESPACE)
                    && object.name == NAME)
        ),
        "component catalog overlaps the reserved profile execution lock"
    );
    Ok(())
}

fn identity(lease: &Lease) -> Result<(&str, &str)> {
    let uid = lease
        .metadata
        .uid
        .as_deref()
        .filter(|value| !value.is_empty())
        .context("lock omitted UID")?;
    let version = lease
        .metadata
        .resource_version
        .as_deref()
        .filter(|value| !value.is_empty())
        .context("lock omitted resource version")?;
    Ok((uid, version))
}

fn validate_current(expected: &Lease, current: &Lease) -> Result<()> {
    ensure!(
        expected.api_version == current.api_version
            && expected.kind == current.kind
            && identity(expected)? == identity(current)?
            && expected.metadata.namespace == current.metadata.namespace
            && expected.metadata.name == current.metadata.name
            && expected.spec.holder_identity == current.spec.holder_identity
            && current.spec.lease_duration_seconds.is_none(),
        "profile execution lock changed; no further mutation is authorized"
    );
    Ok(())
}

fn request(context: &str, arguments: &[&str], body: &impl Serialize) -> Result<Vec<u8>> {
    // Serialize before spawning, then always reap the API client, including EPIPE.
    let bytes = serde_json::to_vec(body)?;
    let mut child = Command::new("kubectl")
        .args(["--context", context, "--request-timeout=10s"])
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let written = child
        .stdin
        .take()
        .context("kubectl has no input pipe")?
        .write_all(&bytes);
    let output = child.wait_with_output()?;
    written.context("writing profile lock request")?;
    ensure!(
        output.status.success(),
        "profile lock API request failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

#[cfg(test)]
mod tests;
