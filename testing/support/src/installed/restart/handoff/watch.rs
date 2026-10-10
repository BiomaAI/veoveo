use super::*;
use serde::Deserialize;
use tokio::io::AsyncReadExt;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub enum Kind {
    Deployments,
    ReplicaSets,
    Pods,
    Services,
    Slices,
}
fn endpoint(target: &InstallationTarget, f: &HandoffFixture, kind: Kind) -> Result<url::Url> {
    let mut url = url::Url::parse("https://kubernetes.invalid/")?;
    let prefix = match kind {
        Kind::Deployments | Kind::ReplicaSets => vec!["apis", "apps", "v1"],
        Kind::Slices => vec!["apis", "discovery.k8s.io", "v1"],
        _ => vec!["api", "v1"],
    };
    let resource = match kind {
        Kind::Deployments => "deployments",
        Kind::ReplicaSets => "replicasets",
        Kind::Pods => "pods",
        Kind::Services => "services",
        Kind::Slices => "endpointslices",
    };
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid Kubernetes API origin"))?
        .extend(prefix)
        .extend(["namespaces", &target.kubernetes.namespace, resource]);
    match kind {
        Kind::Services => {
            url.query_pairs_mut()
                .append_pair("fieldSelector", &format!("metadata.name={}", f.service));
        }
        Kind::Slices => {
            url.query_pairs_mut().append_pair(
                "labelSelector",
                &format!("kubernetes.io/service-name={}", f.service),
            );
        }
        Kind::Pods => {
            url.query_pairs_mut().append_pair(
                "labelSelector",
                &format!(
                    "app.kubernetes.io/component in ({},{})",
                    f.component_a, f.component_b
                ),
            );
        }
        Kind::Deployments | Kind::ReplicaSets => (),
    }
    Ok(url)
}
#[derive(Deserialize)]
struct List {
    metadata: ListMetadata,
    items: Vec<Object>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListMetadata {
    resource_version: String,
}
pub(super) async fn inventory(
    target: &InstallationTarget,
    f: &HandoffFixture,
    kind: Kind,
) -> Result<Inventory> {
    let url = endpoint(target, f, kind)?;
    let mut cmd = tokio::process::Command::new("kubectl");
    cmd.args([
        "--context",
        &target.kubernetes.context,
        "get",
        "--raw",
        &url[url::Position::BeforePath..],
    ])
    .kill_on_drop(true);
    let result = crate::output_async(cmd, Duration::from_secs(15))
        .await
        .map_err(|_| anyhow::anyhow!("handoff inventory command failed"))?;
    decode_inventory(result)
}
fn decode_inventory(result: std::process::Output) -> Result<Inventory> {
    ensure!(
        result.status.success() && result.stdout.len() <= 4 * 1024 * 1024,
        "handoff inventory unavailable or oversized"
    );
    let list: List = serde_json::from_slice(&result.stdout)
        .map_err(|_| anyhow::anyhow!("invalid handoff inventory"))?;
    ensure!(
        !list.metadata.resource_version.is_empty(),
        "handoff inventory lacks version"
    );
    Ok(Inventory {
        resource_version: list.metadata.resource_version,
        objects: list
            .items
            .into_iter()
            .map(|o| (o.metadata.uid, o))
            .collect(),
    })
}

/// Retain the actual cleanup-admitted read until its process has settled.
/// The existing command owner supplies admission, registration and forced cleanup.
pub(super) struct CleanupRead {
    task: Option<tokio::task::JoinHandle<Result<Inventory>>>,
    end: std::time::Instant,
    failed: bool,
}
impl CleanupRead {
    pub(super) fn start(command: std::process::Command, end: std::time::Instant) -> Result<Self> {
        ensure!(
            std::time::Instant::now() < end,
            "cleanup inventory deadline expired"
        );
        let owner = crate::lifecycle::owner::active()
            .ok_or_else(|| anyhow::anyhow!("cleanup inventory requires its original owner"))?;
        ensure!(
            end <= owner.cleanup_end(),
            "cleanup inventory exceeds original owner cap"
        );
        Ok(Self {
            task: Some(tokio::task::spawn_blocking(move || {
                let active = crate::lifecycle::owner::active()
                    .ok_or_else(|| anyhow::anyhow!("cleanup inventory owner no longer active"))?;
                ensure!(
                    std::sync::Arc::ptr_eq(&owner, &active),
                    "cleanup inventory owner replaced"
                );
                ensure!(
                    std::time::Instant::now() < end,
                    "cleanup inventory original deadline expired"
                );
                let output = crate::output_cleanup_command(command)
                    .map_err(|_| anyhow::anyhow!("handoff cleanup inventory command failed"))?;
                decode_inventory(output)
            })),
            end,
            failed: false,
        })
    }
    pub async fn finish(&mut self) -> Result<Inventory> {
        ensure!(
            !self.failed && std::time::Instant::now() < self.end,
            "cleanup inventory original deadline expired"
        );
        let task = self
            .task
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("cleanup inventory already consumed"))?;
        let result = tokio::time::timeout_at(self.end.into(), task).await;
        match result {
            Ok(joined) => {
                self.task.take();
                if std::time::Instant::now() >= self.end {
                    self.failed = true;
                    anyhow::bail!("cleanup inventory original deadline expired");
                }
                joined.map_err(|_| anyhow::anyhow!("cleanup inventory worker failed"))?
            }
            Err(_) => {
                self.failed = true;
                anyhow::bail!("cleanup inventory original deadline expired");
            }
        }
    }
    pub async fn drain(&mut self, end: std::time::Instant) -> Result<()> {
        if let Some(task) = self.task.as_mut() {
            ensure!(
                std::time::Instant::now() < end,
                "cleanup inventory drain deadline expired"
            );
            let joined = tokio::time::timeout_at(end.into(), task)
                .await
                .map_err(|_| anyhow::anyhow!("cleanup inventory drain deadline expired"))?;
            self.task.take();
            joined.map_err(|_| anyhow::anyhow!("cleanup inventory worker failed"))??;
            ensure!(
                std::time::Instant::now() < end,
                "cleanup inventory drain deadline expired"
            );
        }
        Ok(())
    }
}
pub(super) fn cleanup_inventory(
    target: &InstallationTarget,
    f: &HandoffFixture,
    kind: Kind,
    end: std::time::Instant,
) -> Result<CleanupRead> {
    let remaining = end
        .checked_duration_since(std::time::Instant::now())
        .ok_or_else(|| anyhow::anyhow!("cleanup inventory deadline expired"))?
        .min(Duration::from_secs(15));
    let url = endpoint(target, f, kind)?;
    let mut command = std::process::Command::new("kubectl");
    command.args([
        "--context",
        &target.kubernetes.context,
        "--request-timeout",
        &format!("{}ms", remaining.as_millis().max(1)),
        "get",
        "--raw",
        &url[url::Position::BeforePath..],
    ]);
    CleanupRead::start(command, end)
}

#[derive(Deserialize)]
pub(super) struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    pub object: serde_json::Value,
}
pub(super) struct Watch {
    pub kind: Kind,
    child: crate::AsyncChild,
    stdout: tokio::process::ChildStdout,
    pending: Vec<u8>,
    bytes: usize,
}
impl Watch {
    pub fn start(
        target: &InstallationTarget,
        f: &HandoffFixture,
        kind: Kind,
        version: &str,
    ) -> Result<Self> {
        let mut url = endpoint(target, f, kind)?;
        url.query_pairs_mut()
            .append_pair("watch", "true")
            .append_pair("resourceVersion", version)
            .append_pair("allowWatchBookmarks", "true");
        let mut cmd = tokio::process::Command::new("kubectl");
        cmd.args([
            "--context",
            &target.kubernetes.context,
            "get",
            "--raw",
            &url[url::Position::BeforePath..],
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
        let mut child = crate::spawn_async(cmd)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("handoff watch stdout absent"))?;
        Ok(Self {
            kind,
            child,
            stdout,
            pending: vec![],
            bytes: 0,
        })
    }
    #[cfg(test)]
    pub(super) fn native_control(kind: Kind, mut child: crate::AsyncChild) -> Result<Self> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("native watch stdout absent"))?;
        Ok(Self {
            kind,
            child,
            stdout,
            pending: vec![],
            bytes: 0,
        })
    }
    pub async fn next(&mut self) -> Result<Event> {
        loop {
            let mut decoder =
                serde_json::Deserializer::from_slice(&self.pending).into_iter::<Event>();
            match decoder.next() {
                Some(Ok(event)) => {
                    let n = decoder.byte_offset();
                    self.pending.drain(..n);
                    return Ok(event);
                }
                Some(Err(e)) if !e.is_eof() => anyhow::bail!("invalid handoff watch event"),
                _ => (),
            }
            let mut buffer = [0; 8192];
            let count = self
                .stdout
                .read(&mut buffer)
                .await
                .map_err(|_| anyhow::anyhow!("handoff watch read failed"))?;
            ensure!(
                count > 0,
                "handoff watch ended; continuous routing proof is unqualified"
            );
            self.bytes += count;
            ensure!(
                self.bytes <= 8 * 1024 * 1024,
                "handoff watch budget exceeded"
            );
            self.pending.extend_from_slice(&buffer[..count]);
        }
    }
    pub async fn close_until(&mut self, deadline: std::time::Instant) -> Result<()> {
        self.child.cleanup_until(deadline).await?;
        Ok(())
    }
}

pub(super) async fn admit_storage_identity(
    target: &InstallationTarget,
    f: &HandoffFixture,
) -> Result<()> {
    for (kind, name, uid, namespaced) in std::iter::once((
        "namespace",
        target.kubernetes.namespace.as_str(),
        f.namespace_uid,
        false,
    ))
    .chain(
        f.pvc
            .as_ref()
            .map(|p| ("persistentvolumeclaim", p.name.as_str(), p.uid, true)),
    ) {
        let mut cmd = tokio::process::Command::new("kubectl");
        cmd.args(["--context", &target.kubernetes.context]);
        if namespaced {
            cmd.args(["--namespace", &target.kubernetes.namespace]);
        }
        cmd.args(["get", kind, name, "-o", "json"])
            .kill_on_drop(true);
        let result = crate::output_async(cmd, Duration::from_secs(15))
            .await
            .map_err(|_| anyhow::anyhow!("handoff storage identity unavailable"))?;
        ensure!(
            result.status.success() && result.stdout.len() <= 1024 * 1024,
            "handoff storage identity response invalid"
        );
        let object: Object = serde_json::from_slice(&result.stdout)
            .map_err(|_| anyhow::anyhow!("invalid handoff storage identity"))?;
        ensure!(
            object.metadata.uid == uid && object.metadata.deletion_timestamp.is_none(),
            "handoff namespace or authority PVC replaced/deleting"
        );
        if namespaced {
            ensure!(
                object.status.get("phase").and_then(|v| v.as_str()) == Some("Bound")
                    && object
                        .spec
                        .get("accessModes")
                        .and_then(|v| v.as_array())
                        .is_some_and(|v| v.iter().any(|v| v.as_str() == Some("ReadWriteOnce"))),
                "handoff requires admitted bound same-node RWO authority PVC"
            );
        }
    }
    Ok(())
}
