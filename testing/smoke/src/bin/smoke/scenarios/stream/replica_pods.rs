//! Admit two concrete, unchanged Pods from the installed Stream rollout.
use super::super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodIdentity {
    pub name: String,
    pub uid: uuid::Uuid,
    pub controller: uuid::Uuid,
    pub image: String,
    pub image_id: String,
    pub restarts: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    name: String,
    uid: uuid::Uuid,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    deletion_timestamp: Option<String>,
    owner_references: Vec<Owner>,
}
#[derive(Deserialize)]
struct Owner {
    uid: uuid::Uuid,
    kind: String,
    controller: Option<bool>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContainerStatus {
    name: String,
    ready: bool,
    image: String,
    #[serde(rename = "imageID")]
    image_id: String,
    restart_count: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    phase: String,
    container_statuses: Vec<ContainerStatus>,
}
#[derive(Deserialize)]
struct Pod {
    metadata: Metadata,
    status: Status,
}

impl PodIdentity {
    pub(super) fn read(installation: &InstalledTarget, name: &str) -> Result<Self> {
        ensure!(
            !name.is_empty()
                && name.len() <= 253
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "invalid Kubernetes Pod name"
        );
        let kube = &installation.target.kubernetes;
        let data = run_checked(
            Path::new("kubectl"),
            [
                "--context",
                &kube.context,
                "--request-timeout=10s",
                "-n",
                &kube.namespace,
                "get",
                "pod",
                name,
                "-o",
                "json",
            ]
            .map(Into::into),
            [],
        )?;
        Self::admit(serde_json::from_str(&data)?, name)
    }

    fn admit(pod: Pod, name: &str) -> Result<Self> {
        ensure!(pod.metadata.name == name, "Kubernetes returned another Pod");
        ensure!(
            pod.metadata.deletion_timestamp.is_none(),
            "Stream Pod is terminating"
        );
        ensure!(
            pod.metadata
                .labels
                .get("app.kubernetes.io/component")
                .map(String::as_str)
                == Some("stream-mcp"),
            "selected Pod is not a Stream component"
        );
        ensure!(pod.status.phase == "Running", "Stream Pod is not running");
        let container = pod
            .status
            .container_statuses
            .into_iter()
            .find(|c| c.name == "stream-mcp")
            .context("Stream Pod omits its main container")?;
        ensure!(
            container.ready && !container.image_id.is_empty(),
            "Stream container is not ready"
        );
        let owner = pod
            .metadata
            .owner_references
            .into_iter()
            .find(|o| o.controller == Some(true) && o.kind == "ReplicaSet")
            .context("Stream Pod has no ReplicaSet controller")?;
        Ok(Self {
            name: pod.metadata.name,
            uid: pod.metadata.uid,
            controller: owner.uid,
            image: container.image,
            image_id: container.image_id,
            restarts: container.restart_count,
        })
    }
}

pub(super) fn check_pair(writer: &PodIdentity, observer: &PodIdentity) -> Result<()> {
    ensure!(
        writer.uid != observer.uid && writer.name != observer.name,
        "replica qualification requires two different Pods"
    );
    ensure!(
        writer.controller == observer.controller,
        "Stream Pods must belong to the same ReplicaSet"
    );
    ensure!(
        writer.image == observer.image && writer.image_id == observer.image_id,
        "Stream Pods must run the same image"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admits_kubernetes_pod_wire_fields_and_rejects_unready_or_deleting_pods() {
        let wire = serde_json::json!({
            "metadata": {
                "name": "stream-a", "uid": uuid::Uuid::new_v4(),
                "labels": {"app.kubernetes.io/component": "stream-mcp"},
                "ownerReferences": [{"uid": uuid::Uuid::new_v4(), "kind": "ReplicaSet", "controller": true}]
            },
            "status": {"phase": "Running", "containerStatuses": [{
                "name": "stream-mcp", "ready": true, "image": "registry/stream@sha256:abc",
                "imageID": "sha256:abc", "restartCount": 2
            }]}
        });
        let admitted =
            PodIdentity::admit(serde_json::from_value(wire.clone()).unwrap(), "stream-a").unwrap();
        assert_eq!(admitted.image_id, "sha256:abc");
        assert_eq!(admitted.restarts, 2);
        let mut unready = wire.clone();
        unready["status"]["containerStatuses"][0]["ready"] = false.into();
        assert!(PodIdentity::admit(serde_json::from_value(unready).unwrap(), "stream-a").is_err());
        let mut deleting = wire;
        deleting["metadata"]["deletionTimestamp"] = "2026-10-02T00:00:00Z".into();
        assert!(PodIdentity::admit(serde_json::from_value(deleting).unwrap(), "stream-a").is_err());
    }

    fn identity(name: &str) -> PodIdentity {
        PodIdentity {
            name: name.into(),
            uid: uuid::Uuid::new_v4(),
            controller: uuid::Uuid::nil(),
            image: "registry/stream@sha256:abc".into(),
            image_id: "sha256:abc".into(),
            restarts: 0,
        }
    }

    #[test]
    fn rejects_same_process_and_mixed_rollouts() {
        let a = identity("stream-a");
        let b = identity("stream-b");
        check_pair(&a, &b).unwrap();
        assert!(check_pair(&a, &a).is_err());
        let mut changed = b.clone();
        changed.controller = uuid::Uuid::new_v4();
        assert!(check_pair(&a, &changed).is_err());
        let mut changed = b;
        changed.image_id = "sha256:def".into();
        assert!(check_pair(&a, &changed).is_err());
    }
}
