//! The exact Kubernetes resource subset emitted by the manager.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_timestamp: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Secret {
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    #[serde(default)]
    pub immutable: bool,
    #[serde(rename = "type")]
    pub secret_type: String,
    #[serde(default)]
    pub data: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfigMap {
    pub metadata: Metadata,
    #[serde(default)]
    pub immutable: bool,
    #[serde(default)]
    pub data: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pvc {
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: PvcSpec,
    #[serde(default, skip_serializing)]
    pub status: PvcStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvcSpec {
    pub access_modes: Vec<String>,
    pub storage_class_name: String,
    pub resources: StorageResources,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageResources {
    pub requests: StorageQuantity,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageQuantity {
    pub storage: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PvcStatus {
    #[serde(default)]
    pub phase: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deployment {
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: DeploymentSpec,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentSpec {
    pub replicas: u32,
    pub selector: LabelSelector,
    pub strategy: Strategy,
    pub template: PodTemplate,
    pub revision_history_limit: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Strategy {
    #[serde(rename = "type")]
    pub kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelSelector {
    pub match_labels: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PodTemplate {
    pub metadata: Metadata,
    pub spec: PodSpec,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSpec {
    pub service_account_name: String,
    pub automount_service_account_token: bool,
    pub security_context: PodSecurity,
    pub termination_grace_period_seconds: u32,
    pub containers: Vec<Container>,
    pub volumes: Vec<Volume>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodSecurity {
    pub run_as_non_root: bool,
    pub run_as_user: u32,
    pub run_as_group: u32,
    pub fs_group: u32,
    pub seccomp_profile: Seccomp,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Seccomp {
    #[serde(rename = "type")]
    pub kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerSecurity {
    pub allow_privilege_escalation: bool,
    pub read_only_root_filesystem: bool,
    pub capabilities: Capabilities,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Capabilities {
    pub drop: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub name: String,
    pub image: String,
    pub image_pull_policy: String,
    pub args: Vec<String>,
    pub env: Vec<Env>,
    pub resources: ComputeResources,
    pub security_context: ContainerSecurity,
    pub readiness_probe: Probe,
    pub volume_mounts: Vec<VolumeMount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Env {
    pub name: String,
    #[serde(flatten)]
    pub source: EnvValue,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    Literal {
        value: String,
    },
    Reference {
        #[serde(rename = "valueFrom")]
        value_from: EnvSource,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EnvSource {
    SecretKeyRef(SecretKey),
    FieldRef(FieldRef),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretKey {
    pub name: String,
    pub key: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldRef {
    pub field_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComputeResources {
    pub requests: ComputeQuantity,
    pub limits: ComputeQuantity,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComputeQuantity {
    pub cpu: String,
    pub memory: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub exec: Exec,
    pub period_seconds: u32,
    pub timeout_seconds: u32,
    pub failure_threshold: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Exec {
    pub command: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeMount {
    pub name: String,
    pub mount_path: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Volume {
    pub name: String,
    #[serde(flatten)]
    pub source: VolumeSource,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeSource {
    ConfigMap(ConfigVolume),
    PersistentVolumeClaim(ClaimVolume),
    EmptyDir(EmptyDir),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigVolume {
    pub name: String,
    pub items: Vec<ConfigItem>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigItem {
    pub key: String,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimVolume {
    pub claim_name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyDir {
    pub size_limit: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Pod {
    pub metadata: Metadata,
    #[serde(default)]
    pub status: PodStatus,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PodStatus {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub conditions: Vec<Condition>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Condition {
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
}
impl Pod {
    pub fn ready(&self) -> bool {
        self.metadata.deletion_timestamp.is_none()
            && self.status.phase == "Running"
            && self
                .status
                .conditions
                .iter()
                .any(|c| c.kind == "Ready" && c.status == "True")
    }
}
#[derive(Debug, Deserialize)]
pub struct ResourceList<T> {
    pub metadata: ListMetadata,
    pub items: Vec<T>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListMetadata {
    pub resource_version: String,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "object", rename_all = "UPPERCASE")]
pub enum WatchEvent {
    Added(Pod),
    Modified(Pod),
    Deleted(Pod),
    Bookmark(Bookmark),
    Error(ApiStatus),
}
#[derive(Debug, Deserialize)]
pub struct Bookmark {
    pub metadata: Metadata,
}
#[derive(Debug, Deserialize)]
pub struct ApiStatus {
    pub code: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOptions {
    pub api_version: &'static str,
    pub kind: &'static str,
    pub preconditions: Preconditions,
    pub propagation_policy: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preconditions {
    pub uid: String,
    pub resource_version: String,
}
