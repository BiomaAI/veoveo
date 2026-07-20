//! Canonical authority and output-governance contract for related work.
//!
//! A Work Context is the business boundary shared by tasks, recordings,
//! agents, and artifacts. The gateway resolves the caller's membership and
//! invocation mode, then signs the resulting authority into the internal
//! token. Hosted services apply that resolved authority to ownership,
//! provenance, and initial access.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AccessLevel, DataLabelId, DelegationId, GroupId, OAuthClientId, PolicyVersion, Principal,
    PrincipalId, RoleId, TenantId, WorkContextId,
};

/// A principal or group that can own governed data or receive access.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum AccessSubject {
    Principal(PrincipalId),
    Group(GroupId),
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

/// One neutral membership rule supplied by an enterprise installation.
///
/// A rule matches when any populated selector identifies the caller. This
/// keeps enterprise role and group vocabulary in deployment configuration,
/// outside Veoveo's protocol and storage contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkContextMembershipRule {
    pub level: WorkContextMembershipLevel,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub principals: BTreeSet<PrincipalId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub groups: BTreeSet<GroupId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub roles: BTreeSet<RoleId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub oauth_clients: BTreeSet<OAuthClientId>,
}

impl WorkContextMembershipRule {
    pub fn has_selector(&self) -> bool {
        !self.principals.is_empty()
            || !self.groups.is_empty()
            || !self.roles.is_empty()
            || !self.oauth_clients.is_empty()
    }

    pub fn matches(&self, principal: &Principal, oauth_client: &OAuthClientId) -> bool {
        self.principals.contains(&principal.id)
            || !self.groups.is_disjoint(&principal.groups)
            || !self.roles.is_disjoint(&principal.roles)
            || self.oauth_clients.contains(oauth_client)
    }
}

/// Initial discretionary policy stamped on every output in a Work Context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkContextGrant {
    pub subject: AccessSubject,
    pub level: AccessLevel,
}

/// Immutable output defaults resolved with an invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkContextOutputPolicy {
    pub owner: AccessSubject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_grants: Vec<WorkContextGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataLabelId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
}

/// One configured Work Context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkContextDefinition {
    pub id: WorkContextId,
    pub tenant: TenantId,
    pub title: String,
    pub policy_revision: PolicyVersion,
    pub output_policy: WorkContextOutputPolicy,
    pub memberships: Vec<WorkContextMembershipRule>,
}

impl WorkContextDefinition {
    pub fn membership_for(
        &self,
        principal: &Principal,
        oauth_client: &OAuthClientId,
    ) -> Option<WorkContextMembershipLevel> {
        self.memberships
            .iter()
            .filter(|rule| rule.matches(principal, oauth_client))
            .map(|rule| rule.level)
            .max()
    }
}

/// How the current actor obtained authority to perform the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvocationMode {
    Direct,
    Delegated,
    Automated,
}

/// Provenance retained across synchronous and asynchronous execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum InvocationProvenance {
    Direct {
        initiator: PrincipalId,
    },
    Delegated {
        initiator: PrincipalId,
        delegation_id: DelegationId,
    },
    Automated,
}

impl InvocationProvenance {
    pub fn mode(&self) -> InvocationMode {
        match self {
            Self::Direct { .. } => InvocationMode::Direct,
            Self::Delegated { .. } => InvocationMode::Delegated,
            Self::Automated => InvocationMode::Automated,
        }
    }

    pub fn initiator(&self) -> Option<&PrincipalId> {
        match self {
            Self::Direct { initiator } | Self::Delegated { initiator, .. } => Some(initiator),
            Self::Automated => None,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PrincipalAssurance, PrincipalKind, ScopeName, TokenIssuer, TokenSubject};

    fn principal() -> Principal {
        Principal {
            id: PrincipalId::new("issuer#subject").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("issuer").unwrap(),
            subject: TokenSubject::new("subject").unwrap(),
            tenant: Some(TenantId::new("tenant").unwrap()),
            groups: BTreeSet::from([GroupId::new("flight").unwrap()]),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::from([RoleId::new("operator").unwrap()]),
            scopes: BTreeSet::<ScopeName>::new(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::<PrincipalAssurance>::new(),
            authenticated_at: None,
        }
    }

    #[test]
    fn highest_matching_membership_wins() {
        let context = WorkContextDefinition {
            id: WorkContextId::new("mission").unwrap(),
            tenant: TenantId::new("tenant").unwrap(),
            title: "Mission".into(),
            policy_revision: PolicyVersion::new("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Group(GroupId::new("flight").unwrap()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            memberships: vec![
                WorkContextMembershipRule {
                    level: WorkContextMembershipLevel::Viewer,
                    principals: BTreeSet::new(),
                    groups: BTreeSet::from([GroupId::new("flight").unwrap()]),
                    roles: BTreeSet::new(),
                    oauth_clients: BTreeSet::new(),
                },
                WorkContextMembershipRule {
                    level: WorkContextMembershipLevel::Custodian,
                    principals: BTreeSet::new(),
                    groups: BTreeSet::new(),
                    roles: BTreeSet::from([RoleId::new("operator").unwrap()]),
                    oauth_clients: BTreeSet::new(),
                },
            ],
        };
        assert_eq!(
            context.membership_for(&principal(), &OAuthClientId::new("console").unwrap()),
            Some(WorkContextMembershipLevel::Custodian)
        );
    }
}
