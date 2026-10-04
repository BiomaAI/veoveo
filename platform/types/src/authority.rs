//! Resolved invocation authority and output policy, independent of protocol transport.
//! Values carry claims; authentication and policy enforcement establish their authority.
use crate::{
    AccessSubject, DataLabelId, InvocationProvenance, PolicyVersion, TenantId, WorkContextId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A capability level. Ordered `Read < Write < Admin`, so `min` yields the
/// lesser privilege — exactly what capping a group role by a grant level needs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    Read,
    Write,
    Admin,
}

impl AccessLevel {
    /// True when `self` is sufficient for a request that needs `required`.
    pub fn allows(self, required: AccessLevel) -> bool {
        self >= required
    }
}

/// A member's authority inside one Work Context.
///
/// Ordering is intentional. It lets an enforcement point compare the current
/// membership with the minimum level required by an operation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkContextMembershipLevel {
    Viewer,
    Contributor,
    Custodian,
    Owner,
}

impl WorkContextMembershipLevel {
    pub fn allows(self, required: Self) -> bool {
        self >= required
    }

    pub fn artifact_access(self) -> AccessLevel {
        match self {
            Self::Viewer => AccessLevel::Read,
            Self::Contributor => AccessLevel::Write,
            Self::Custodian | Self::Owner => AccessLevel::Admin,
        }
    }
}

/// Initial discretionary policy stamped on every output in a Work Context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkContextGrant {
    pub subject: AccessSubject,
    pub level: AccessLevel,
}

/// Immutable output defaults resolved with an invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkContextOutputPolicy {
    pub owner: AccessSubject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_grants: Vec<WorkContextGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataLabelId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
}

/// Gateway-resolved authority signed into every internal service token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InvocationAuthority {
    pub work_context: WorkContextId,
    pub tenant: TenantId,
    pub membership: WorkContextMembershipLevel,
    pub policy_revision: PolicyVersion,
    pub output_policy: WorkContextOutputPolicy,
    pub provenance: InvocationProvenance,
}

impl InvocationAuthority {
    pub fn artifact_access(&self) -> AccessLevel {
        self.membership.artifact_access()
    }
}
