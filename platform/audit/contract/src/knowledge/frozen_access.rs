//! Frozen Audit access bytes, distinct from the public Knowledge extension.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_knowledge_extension::{AccessDescriptor, ReadGrant, ReadPolicy};
use veoveo_types::{AccessSubject, DataLabelId, GatewayProfileId, TenantId, WorkContextId};

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "AccessDescriptor")]
pub(super) struct FrozenAccess {
    tenant: TenantId,
    work_context: WorkContextId,
    read_policy: FrozenPolicy,
    owner: AccessSubject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    grants: Vec<ReadGrant>,
    data_labels: Vec<DataLabelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<DateTime<Utc>>,
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
#[schemars(rename = "ReadPolicy")]
enum FrozenPolicy {
    Tenant {},
    Subjects {},
    WorkContext {},
    SelectedWorkContext {},
    SelectedWorkContextMembers {},
    SubjectsInContext {
        #[serde(skip_serializing_if = "Option::is_none")]
        profile: Option<GatewayProfileId>,
    },
}
impl From<ReadPolicy> for FrozenPolicy {
    fn from(value: ReadPolicy) -> Self {
        match value {
            ReadPolicy::Tenant {} => Self::Tenant {},
            ReadPolicy::Subjects {} => Self::Subjects {},
            ReadPolicy::WorkContext {} => Self::WorkContext {},
            ReadPolicy::SelectedWorkContext {} => Self::SelectedWorkContext {},
            ReadPolicy::SelectedWorkContextMembers {} => Self::SelectedWorkContextMembers {},
            ReadPolicy::SubjectsInContext { profile } => Self::SubjectsInContext { profile },
        }
    }
}
impl From<FrozenPolicy> for ReadPolicy {
    fn from(value: FrozenPolicy) -> Self {
        match value {
            FrozenPolicy::Tenant {} => Self::Tenant {},
            FrozenPolicy::Subjects {} => Self::Subjects {},
            FrozenPolicy::WorkContext {} => Self::WorkContext {},
            FrozenPolicy::SelectedWorkContext {} => Self::SelectedWorkContext {},
            FrozenPolicy::SelectedWorkContextMembers {} => Self::SelectedWorkContextMembers {},
            FrozenPolicy::SubjectsInContext { profile } => Self::SubjectsInContext { profile },
        }
    }
}
impl From<AccessDescriptor> for FrozenAccess {
    fn from(value: AccessDescriptor) -> Self {
        Self {
            tenant: value.tenant,
            work_context: value.work_context,
            read_policy: value.read_policy.into(),
            owner: value.owner,
            grants: value.grants,
            data_labels: value.data_labels,
            expires_at: value.expires_at,
        }
    }
}
impl From<FrozenAccess> for AccessDescriptor {
    fn from(value: FrozenAccess) -> Self {
        Self {
            tenant: value.tenant,
            work_context: value.work_context,
            read_policy: value.read_policy.into(),
            owner: value.owner,
            grants: value.grants,
            data_labels: value.data_labels,
            expires_at: value.expires_at,
        }
    }
}
pub(super) fn serialize<S: serde::Serializer>(
    value: &Option<AccessDescriptor>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.clone().map(FrozenAccess::from).serialize(serializer)
}
pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Option<AccessDescriptor>, D::Error> {
    Option::<FrozenAccess>::deserialize(decoder).map(|value| value.map(Into::into))
}
