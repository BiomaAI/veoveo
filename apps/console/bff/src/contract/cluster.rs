//! Browser projection of Kubernetes inventory responses.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum ClusterOrchestrator {
    #[vocabulary(rename = "Kubernetes")]
    Kubernetes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum ClusterWorkloadKind {
    #[vocabulary(rename = "Deployment")]
    Deployment,
    #[vocabulary(rename = "StatefulSet")]
    StatefulSet,
    #[vocabulary(rename = "Job")]
    Job,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSnapshot {
    pub orchestrator: ClusterOrchestrator,
    pub namespace: String,
    pub generated_at: DateTime<Utc>,
    pub workloads: Vec<ClusterWorkload>,
    pub pods: Vec<ClusterPod>,
    pub services: Vec<ClusterService>,
    pub storage: Vec<ClusterStorage>,
    pub ingresses: Vec<ClusterIngress>,
    pub network_policies: Vec<String>,
    pub disruption_budgets: Vec<String>,
    pub config_maps: Vec<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterWorkload {
    pub name: String,
    pub kind: ClusterWorkloadKind,
    pub desired: u32,
    pub ready: u32,
    pub available: u32,
    pub images: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterPod {
    pub name: String,
    pub component: Option<String>,
    pub phase: String,
    pub ready: u32,
    pub containers: u32,
    pub restarts: u32,
    pub node: Option<String>,
    pub images: Vec<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterService {
    pub name: String,
    pub kind: String,
    pub cluster_ip: Option<String>,
    pub ports: Vec<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterStorage {
    pub name: String,
    pub phase: String,
    pub requested: Option<String>,
    pub capacity: Option<String>,
    pub storage_class: Option<String>,
    pub access_modes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterIngress {
    pub name: String,
    pub class_name: Option<String>,
    pub hosts: Vec<String>,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(ClusterSnapshot)
}
