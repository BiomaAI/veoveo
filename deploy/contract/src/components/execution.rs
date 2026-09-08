//! Successful execution evidence; API request auditing remains a separate observer.
use super::{AtomicTarget, ComponentId, ComponentMutationPlan, ObjectIdentity};
use serde::{Deserialize, Serialize};
use veoveo_extension_contract::ArtifactDigest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentSelection {
    All,
    Exact(std::collections::BTreeSet<ComponentId>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationReceipt {
    pub schema_version: String,
    pub plan: ComponentMutationPlan,
    pub operations: Vec<UnitExecution>,
    pub unselected_before: UnselectedState,
    pub unselected_after: UnselectedState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitExecution {
    pub component: ComponentId,
    pub target: AtomicTarget,
    pub outcome: UnitExecutionOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitExecutionOutcome {
    Reused,
    Applied,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnselectedState {
    pub objects: Vec<ObjectSnapshot>,
    pub releases: Vec<ReleaseSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectSnapshot {
    pub identity: ObjectIdentity,
    pub state: Option<ObservedObjectVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedObjectVersion {
    pub uid: String,
    pub resource_version: String,
    /// Hash of the complete API object, including status and resource version.
    pub digest: ArtifactDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseSnapshot {
    pub target: AtomicTarget,
    pub state: Option<ObservedReleaseVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedReleaseVersion {
    pub revision: u64,
    pub status: String,
    pub chart: String,
    pub app_version: String,
}
