//! Typed Kubernetes watch and persisted kernel observations for one owned agent.
use super::super::{fixture::Fixture, process};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_platform_store::agent_management::instances::{
    ManagedAgentIdentity, ManagedAgentPublicKey,
};
use veoveo_platform_store::{AgentEpisodeRecord, AgentRecord, PlatformStore};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    pub name: String,
    pub uid: Uuid,
    pub resource_version: String,
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
    pub creation_timestamp: DateTime<Utc>,
    pub deletion_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finalizers: Vec<String>,
}
#[derive(Clone, Deserialize)]
pub(super) struct Pod {
    pub metadata: Metadata,
    #[serde(default)]
    pub status: PodStatus,
}
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodStatus {
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub container_statuses: Vec<ContainerStatus>,
}
#[derive(Clone, Deserialize)]
pub(super) struct Condition {
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContainerStatus {
    #[serde(rename = "imageID")]
    pub image_id: String,
}
impl Pod {
    pub fn database_credential_revision(&self) -> Result<&str> {
        self.metadata
            .annotations
            .get("veoveo.ai/database-credential-revision")
            .map(String::as_str)
            .context("managed Pod credential revision annotation absent")
    }
    pub fn ready(&self) -> bool {
        self.metadata.deletion_timestamp.is_none()
            && self
                .status
                .conditions
                .iter()
                .any(|c| c.kind == "Ready" && c.status == "True")
    }
}
#[derive(Deserialize)]
pub(super) struct Object {
    pub metadata: Metadata,
}
#[derive(Deserialize)]
pub(super) struct PodList {
    pub metadata: ListMetadata,
    pub items: Vec<Pod>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListMetadata {
    pub resource_version: String,
}
#[derive(Deserialize)]
#[serde(tag = "type", content = "object")]
enum PodEvent {
    #[serde(rename = "ADDED")]
    Added(Pod),
    #[serde(rename = "MODIFIED")]
    Modified(Pod),
    #[serde(rename = "DELETED")]
    Deleted(Pod),
    #[serde(rename = "BOOKMARK")]
    Bookmark(Bookmark),
    #[serde(rename = "ERROR")]
    Error(WatchFailure),
}
#[derive(Deserialize)]
struct Bookmark {
    metadata: ListMetadata,
}
#[derive(Deserialize)]
struct WatchFailure {
    code: u16,
}
/// Kubernetes emits a stream of JSON objects; reads may split any object.
#[derive(Default)]
struct WatchBuffer {
    pending: Vec<u8>,
}
impl WatchBuffer {
    fn feed(
        &mut self,
        bytes: &[u8],
        pods: &mut BTreeMap<Uuid, Pod>,
        deleted: &mut Vec<Uuid>,
        added: &mut Vec<Metadata>,
        retired: &mut Vec<Uuid>,
    ) -> Result<()> {
        self.pending.extend_from_slice(bytes);
        let mut events =
            serde_json::Deserializer::from_slice(&self.pending).into_iter::<PodEvent>();
        for event in events.by_ref() {
            match event {
                Ok(PodEvent::Added(p)) => {
                    ensure!(
                        !p.metadata.resource_version.is_empty(),
                        "watch object resourceVersion absent"
                    );
                    ensure!(
                        pods.is_empty(),
                        "replacement appeared before old workload deletion"
                    );
                    added.push(p.metadata.clone());
                    record_retirement(&p.metadata, retired);
                    pods.insert(p.metadata.uid, p);
                }
                Ok(PodEvent::Modified(p)) => {
                    ensure!(
                        !p.metadata.resource_version.is_empty(),
                        "watch object resourceVersion absent"
                    );
                    ensure!(
                        pods.contains_key(&p.metadata.uid),
                        "watch modified an unobserved workload"
                    );
                    record_retirement(&p.metadata, retired);
                    pods.insert(p.metadata.uid, p);
                }
                Ok(PodEvent::Deleted(p)) => {
                    ensure!(
                        !p.metadata.resource_version.is_empty(),
                        "watch object resourceVersion absent"
                    );
                    ensure!(
                        pods.remove(&p.metadata.uid).is_some(),
                        "watch deleted an unobserved workload"
                    );
                    record_retirement(&p.metadata, retired);
                    deleted.push(p.metadata.uid);
                }
                Ok(PodEvent::Bookmark(bookmark)) => ensure!(
                    !bookmark.metadata.resource_version.is_empty(),
                    "watch bookmark resourceVersion absent"
                ),
                Ok(PodEvent::Error(failure)) => anyhow::bail!(
                    "workload watch rejected with status {}; gaps cannot prove drain",
                    failure.code
                ),
                Err(error) if error.is_eof() => break,
                Err(_) => anyhow::bail!("workload watch decode failed; gaps cannot prove drain"),
            }
        }
        let consumed = events.byte_offset();
        self.pending.drain(..consumed);
        ensure!(
            self.pending.len() < 1024 * 1024,
            "incomplete workload watch object exceeded 1 MiB"
        );
        Ok(())
    }
}
fn record_retirement(metadata: &Metadata, retired: &mut Vec<Uuid>) {
    if metadata.deletion_timestamp.is_some()
        && metadata
            .finalizers
            .iter()
            .any(|value| value == "foregroundDeletion")
        && !retired.contains(&metadata.uid)
    {
        retired.push(metadata.uid);
    }
}
#[derive(Clone, Copy)]
pub(super) enum WorkloadKind {
    Pods,
    Deployments,
}
impl WorkloadKind {
    fn name(self) -> &'static str {
        match self {
            Self::Pods => "pods",
            Self::Deployments => "deployments",
        }
    }
    fn endpoint(self, namespace: &str, workload: &str) -> Result<url::Url> {
        let mut endpoint = url::Url::parse(match self {
            Self::Pods => "https://kubernetes.invalid/api/v1/namespaces/",
            Self::Deployments => "https://kubernetes.invalid/apis/apps/v1/namespaces/",
        })?;
        endpoint
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Kubernetes API cannot hold path segments"))?
            .pop_if_empty()
            .push(namespace)
            .push(self.name());
        endpoint.query_pairs_mut().append_pair(
            "labelSelector",
            &format!("veoveo.ai/managed-agent={workload}"),
        );
        Ok(endpoint)
    }
    fn list_path(self, namespace: &str, workload: &str) -> Result<String> {
        let endpoint = self.endpoint(namespace, workload)?;
        Ok(endpoint[url::Position::BeforePath..].to_owned())
    }
    fn watch_path(self, namespace: &str, workload: &str, version: &str) -> Result<String> {
        ensure!(!version.is_empty(), "workload watch resourceVersion absent");
        let mut endpoint = self.endpoint(namespace, workload)?;
        endpoint
            .query_pairs_mut()
            .append_pair("watch", "true")
            .append_pair("resourceVersion", version)
            .append_pair("timeoutSeconds", "550");
        Ok(endpoint[url::Position::BeforePath..].to_owned())
    }
    fn observer(self) -> &'static str {
        match self {
            Self::Pods => "managed Pod watch",
            Self::Deployments => "managed Deployment watch",
        }
    }
}
impl PodList {
    fn admit(bytes: &[u8]) -> Result<Self> {
        let list: Self = serde_json::from_slice(bytes)?;
        ensure!(
            !list.metadata.resource_version.is_empty(),
            "workload list resourceVersion absent"
        );
        ensure!(list.items.len() <= 1, "initial workload inventory overlaps");
        ensure!(
            list.items
                .iter()
                .all(|p| !p.metadata.resource_version.is_empty()),
            "listed workload resourceVersion absent"
        );
        Ok(list)
    }
}

pub(super) struct PodWatch {
    child: process::Background,
    buffer: WatchBuffer,
    pub pods: BTreeMap<Uuid, Pod>,
    pub deleted: Vec<Uuid>,
    pub added: Vec<Metadata>,
    pub retired: Vec<Uuid>,
}
impl PodWatch {
    pub fn start(fixture: &Fixture, namespace: &str, workload: &str) -> Result<Self> {
        Self::start_resource(fixture, namespace, workload, WorkloadKind::Pods)
    }
    pub fn start_resource(
        fixture: &Fixture,
        namespace: &str,
        workload: &str,
        kind: WorkloadKind,
    ) -> Result<Self> {
        // Normal kubectl output flattens empty lists and discards their resourceVersion.
        // Read the single-resource API response intact before starting the watch at its revision.
        let path = kind.list_path(namespace, workload)?;
        let list = PodList::admit(&process::checked(
            fixture.kubectl_in(namespace).args(["get", "--raw", &path]),
            20,
        )?)?;
        let mut retired = Vec::new();
        for p in &list.items {
            record_retirement(&p.metadata, &mut retired);
        }
        let watch = kind.watch_path(namespace, workload, &list.metadata.resource_version)?;
        let child = process::Background::start(
            fixture
                .kubectl_in(namespace)
                .args(["get", "--raw", &watch, "--request-timeout=0"]),
            600,
            kind.observer(),
        )?;
        Ok(Self {
            child,
            buffer: WatchBuffer::default(),
            pods: list
                .items
                .into_iter()
                .map(|p| (p.metadata.uid, p))
                .collect(),
            deleted: vec![],
            added: vec![],
            retired,
        })
    }
    pub fn advance(&mut self) -> Result<()> {
        self.buffer.feed(
            &self.child.read()?,
            &mut self.pods,
            &mut self.deleted,
            &mut self.added,
            &mut self.retired,
        )
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Ready {
    pub registration: surrealdb::types::RecordId,
    pub revision: surrealdb::types::RecordId,
    pub identity: ManagedAgentIdentity,
    pub instance_generation: i64,
    pub active_generation: i64,
    pub runtime_id: surrealdb::types::RecordId,
    pub deployment_uid: Uuid,
    pub pod_uid: Uuid,
    pub signing_secret_uid: Uuid,
    pub public_jwk: ManagedAgentPublicKey,
    pub pvc_uid: Uuid,
    pub content_sha256: String,
    pub lease_owner: String,
    pub lease_fence: i64,
    pub renewals: Vec<DateTime<Utc>>,
    pub database_credential_revision: String,
    pub kernel_image_id: String,
}
pub(super) async fn zero_episodes(store: &PlatformStore) -> Result<Vec<AgentRecord>> {
    let mut response = store
        .client()
        .query("SELECT * FROM agent; SELECT * FROM agent_episode;")
        .await?
        .check()?;
    let agents: Vec<AgentRecord> = response.take(0)?;
    let episodes: Vec<AgentEpisodeRecord> = response.take(1)?;
    ensure!(
        episodes.is_empty(),
        "fixture kernel persisted an episode; model non-dispatch is not established"
    );
    ensure!(
        agents
            .iter()
            .all(|a| a.next_episode_sequence == 1 && a.last_episode.is_none()),
        "fixture kernel episode sequence advanced"
    );
    Ok(agents)
}
pub(super) fn object(fixture: &Fixture, namespace: &str, kind: &str, name: &str) -> Result<Object> {
    serde_json::from_slice(&process::checked(
        fixture
            .kubectl_in(namespace)
            .args(["get", kind, name, "-o=json"]),
        20,
    )?)
    .context("decode owned Kubernetes object metadata")
}
pub(super) fn witness(
    fixture: &Fixture,
    namespace: &str,
    pod: &str,
    create: bool,
) -> Result<String> {
    let script = if create {
        "printf '%s' 'veoveo-credential-recovery-content-v1' > /var/lib/veoveo/agent/credential-recovery-witness; cat /var/lib/veoveo/agent/credential-recovery-witness"
    } else {
        "cat /var/lib/veoveo/agent/credential-recovery-witness"
    };
    let bytes = process::checked(
        fixture
            .kubectl_in(namespace)
            .args(["exec", pod, "--", "/bin/sh", "-c", script]),
        20,
    )?;
    ensure!(
        bytes == b"veoveo-credential-recovery-content-v1",
        "retained memory content witness changed"
    );
    use sha2::Digest;
    Ok(hex::encode(sha2::Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn raw_workload_lists_preserve_empty_inventory_revision() -> Result<()> {
        for (kind, route) in [
            (WorkloadKind::Pods, "/api/v1/namespaces/agents/pods"),
            (
                WorkloadKind::Deployments,
                "/apis/apps/v1/namespaces/agents/deployments",
            ),
        ] {
            let path = kind.list_path("agents", "agent-fixture")?;
            assert_eq!(
                path,
                format!("{route}?labelSelector=veoveo.ai%2Fmanaged-agent%3Dagent-fixture")
            );
            let watch = kind.watch_path("agents", "agent-fixture", "4264")?;
            assert_eq!(
                watch,
                format!("{path}&watch=true&resourceVersion=4264&timeoutSeconds=550")
            );
            assert!(kind.watch_path("agents", "agent-fixture", "").is_err());
            let encoded = kind.list_path("namespace/reserved", "agent +reserved")?;
            let endpoint = url::Url::parse(&format!("https://kubernetes.invalid{encoded}"))?;
            assert!(endpoint.path().contains("namespace%2Freserved"));
            assert_eq!(
                endpoint.query_pairs().collect::<Vec<_>>(),
                [(
                    "labelSelector".into(),
                    "veoveo.ai/managed-agent=agent +reserved".into()
                )]
            );
            let empty = PodList::admit(br#"{"metadata":{"resourceVersion":"3074"},"items":[]}"#)?;
            assert!(empty.items.is_empty());
            assert_eq!(empty.metadata.resource_version, "3074");
        }
        for invalid in [
            br#"{"metadata":{"resourceVersion":""},"items":[]}"#.as_slice(),
            br#"{"metadata":{},"items":[]}"#.as_slice(),
        ] {
            assert!(PodList::admit(invalid).is_err());
        }
        Ok(())
    }

    fn event(kind: &str, uid: Uuid) -> Value {
        json!({"type":kind,"object":{
            "metadata":{"name":"managed-fixture","uid":uid,"resourceVersion":"10",
                "creationTimestamp":"2026-10-04T12:00:00Z"},
            "status":{"conditions":[{"type":"Ready","status":"True"}],
                "containerStatuses":[{"imageID":"containerd://sha256:fixture"}]}
        }})
    }
    #[derive(Default)]
    struct Inventory {
        buffer: WatchBuffer,
        pods: BTreeMap<Uuid, Pod>,
        deleted: Vec<Uuid>,
        added: Vec<Metadata>,
        retired: Vec<Uuid>,
    }
    impl Inventory {
        fn feed(&mut self, bytes: &[u8]) -> Result<()> {
            self.buffer.feed(
                bytes,
                &mut self.pods,
                &mut self.deleted,
                &mut self.added,
                &mut self.retired,
            )
        }
        fn observe(&mut self, event: Value) -> Result<()> {
            self.feed(&serde_json::to_vec(&event)?)
        }
    }

    #[test]
    fn fragmented_ordered_replacement_preserves_retirement_and_image_identity() {
        let old = Uuid::from_u128(1);
        let new = Uuid::from_u128(2);
        let mut inventory = Inventory::default();
        inventory.observe(event("ADDED", old)).unwrap();
        assert!(inventory.pods[&old].ready());
        assert_eq!(
            inventory.pods[&old].status.container_statuses[0].image_id,
            "containerd://sha256:fixture"
        );
        let mut retiring = event("MODIFIED", old);
        retiring["object"]["metadata"]["deletionTimestamp"] = json!("2026-10-04T12:00:01Z");
        retiring["object"]["metadata"]["finalizers"] = json!(["foregroundDeletion"]);
        let stream = [
            retiring.clone(),
            retiring,
            event("DELETED", old),
            event("ADDED", new),
        ]
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
        for byte in stream.as_bytes() {
            inventory.feed(&[*byte]).unwrap();
        }
        assert!(inventory.buffer.pending.is_empty());
        assert_eq!(
            inventory.pods.keys().copied().collect::<Vec<_>>(),
            vec![new]
        );
        assert_eq!(inventory.deleted, vec![old]);
        assert_eq!(inventory.retired, vec![old]);
        assert_eq!(inventory.added.len(), 2);
        assert_eq!(inventory.added[1].uid, new);
        assert_eq!(
            inventory.added[1].creation_timestamp.to_rfc3339(),
            "2026-10-04T12:00:00+00:00"
        );
    }

    #[test]
    fn replacement_order_is_checked_inside_each_read_buffer() {
        let old = Uuid::from_u128(1);
        let new = Uuid::from_u128(2);
        for reverse in [false, true] {
            let mut inventory = Inventory::default();
            inventory.observe(event("ADDED", old)).unwrap();
            let stream = if reverse {
                [event("ADDED", new), event("DELETED", old)]
            } else {
                [event("DELETED", old), event("ADDED", new)]
            };
            let bytes = stream
                .iter()
                .map(|e| serde_json::to_string(e).unwrap())
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(inventory.feed(bytes.as_bytes()).is_err(), reverse);
        }
        let mut inventory = Inventory::default();
        let mut old_event = event("ADDED", old);
        old_event["object"]["metadata"]["deletionTimestamp"] = json!("2026-10-04T12:00:01Z");
        inventory.observe(old_event).unwrap();
        assert!(!inventory.pods[&old].ready());
        assert!(inventory.observe(event("ADDED", new)).is_err());
    }

    #[test]
    fn error_events_and_non_eof_decode_failures_reject_watch_gaps() {
        for code in [410, 403, 500] {
            assert!(
                Inventory::default()
                    .observe(json!({"type":"ERROR","object":{"code":code}}))
                    .is_err()
            );
        }
        for malformed in [
            b"{\"type\":!}".as_slice(),
            b"{\"type\":\"UNKNOWN\",\"object\":{}}",
            b"null",
        ] {
            assert!(Inventory::default().feed(malformed).is_err());
        }
        let mut inventory = Inventory::default();
        inventory.feed(b"{\"type\":").unwrap();
        assert!(inventory.feed(b"!}").is_err());
        assert!(
            Inventory::default()
                .observe(event("DELETED", Uuid::from_u128(9)))
                .is_err()
        );
        assert!(
            Inventory::default()
                .observe(event("MODIFIED", Uuid::from_u128(9)))
                .is_err()
        );
    }

    #[test]
    fn every_object_and_bookmark_requires_a_resource_version() {
        for kind in ["ADDED", "MODIFIED", "DELETED"] {
            for absent in [false, true] {
                let mut inventory = Inventory::default();
                let uid = Uuid::from_u128(1);
                if kind != "ADDED" {
                    inventory.observe(event("ADDED", uid)).unwrap();
                }
                let mut malformed = event(kind, uid);
                if absent {
                    malformed["object"]["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .remove("resourceVersion");
                } else {
                    malformed["object"]["metadata"]["resourceVersion"] = json!("");
                }
                assert!(inventory.observe(malformed).is_err());
            }
        }
        for metadata in [json!({}), json!({"resourceVersion":""})] {
            assert!(
                Inventory::default()
                    .observe(json!({"type":"BOOKMARK","object":{"metadata":metadata}}))
                    .is_err()
            );
        }
        Inventory::default()
            .observe(json!({"type":"BOOKMARK","object":{"metadata":{"resourceVersion":"11"}}}))
            .unwrap();
    }

    #[test]
    fn credential_revision_requires_the_observed_pod_annotation() {
        let mut deployment = event("ADDED", Uuid::from_u128(1))["object"].clone();
        deployment["metadata"]["annotations"] =
            json!({"veoveo.ai/database-credential-revision":"new-revision"});
        deployment["spec"] = json!({"template":{"metadata":{"annotations":{"veoveo.ai/database-credential-revision":"new-revision"}}}});
        let deployment: Object = serde_json::from_value(deployment).unwrap();
        assert_eq!(
            deployment.metadata.annotations["veoveo.ai/database-credential-revision"],
            "new-revision"
        );
        let mut pod = event("ADDED", Uuid::from_u128(2))["object"].clone();
        let absent: Pod = serde_json::from_value(pod.clone()).unwrap();
        assert!(absent.database_credential_revision().is_err());
        pod["metadata"]["annotations"] =
            json!({"veoveo.ai/database-credential-revision":"old-revision"});
        let pod: Pod = serde_json::from_value(pod).unwrap();
        assert_eq!(pod.database_credential_revision().unwrap(), "old-revision");
        assert_ne!(
            pod.database_credential_revision().unwrap(),
            deployment.metadata.annotations["veoveo.ai/database-credential-revision"]
        );
    }

    #[test]
    fn pending_fragment_is_bounded_and_deployment_status_can_be_absent() {
        let mut bytes = b"{\"type\":\"ADDED\",\"object\":{\"metadata\":{\"name\":\"".to_vec();
        bytes.resize(1024 * 1024, b'x');
        assert!(Inventory::default().feed(&bytes).is_err());
        let mut deployment = event("ADDED", Uuid::from_u128(1));
        deployment["object"]
            .as_object_mut()
            .unwrap()
            .remove("status");
        let mut inventory = Inventory::default();
        inventory.observe(deployment).unwrap();
        assert!(!inventory.pods.values().next().unwrap().ready());
    }
}
