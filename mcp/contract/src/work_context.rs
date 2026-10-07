//! Work Context configuration and membership matching for protocol callers.
//!
//! A Work Context is the business boundary shared by tasks, recordings,
//! agents, and artifacts. The gateway resolves the caller's membership and
//! invocation mode, then signs the resulting authority into the internal
//! token. Hosted services apply that resolved authority to ownership,
//! provenance, and initial access. Resolved authority values belong to `veoveo-types`.

use std::collections::BTreeSet;
use veoveo_types::{WorkContextMembershipLevel, WorkContextOutputPolicy};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{OAuthClientId, Principal};
use veoveo_types::{GroupId, PolicyVersion, PrincipalId, RoleId, TenantId, WorkContextId};

/// One neutral membership rule supplied by an enterprise installation.
///
/// A rule matches when any populated selector identifies the caller. This
/// keeps enterprise role and group vocabulary in deployment configuration,
/// outside Veoveo's protocol and storage contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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

/// One configured Work Context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct WorkContextDefinition {
    pub id: WorkContextId,
    pub tenant: TenantId,
    pub title: String,
    pub policy_revision: PolicyVersion,
    #[serde(with = "output_policy_config")]
    #[schemars(with = "OutputPolicyConfig")]
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

/// Installation wire defaults; signed invocation output policy keeps its native format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputPolicyConfig {
    pub owner: veoveo_types::AccessSubject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_grants: Vec<veoveo_types::WorkContextGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<veoveo_types::DataLabelId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<veoveo_types::DataLabelId>,
}
impl From<OutputPolicyConfig> for WorkContextOutputPolicy {
    fn from(value: OutputPolicyConfig) -> Self {
        Self {
            owner: value.owner,
            initial_grants: value.initial_grants,
            classification: value.classification,
            data_labels: value.data_labels,
        }
    }
}
impl From<WorkContextOutputPolicy> for OutputPolicyConfig {
    fn from(value: WorkContextOutputPolicy) -> Self {
        Self {
            owner: value.owner,
            initial_grants: value.initial_grants,
            classification: value.classification,
            data_labels: value.data_labels,
        }
    }
}
mod output_policy_config {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        value: &WorkContextOutputPolicy,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        OutputPolicyConfig::from(value.clone()).serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        decoder: D,
    ) -> Result<WorkContextOutputPolicy, D::Error> {
        OutputPolicyConfig::deserialize(decoder).map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PrincipalAssurance, PrincipalKind, TokenIssuer, TokenSubject};
    use veoveo_types::{AccessSubject, ScopeName};

    fn principal() -> Principal {
        Principal {
            id: PrincipalId::parse("issuer#subject").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("issuer").unwrap(),
            subject: TokenSubject::parse("subject").unwrap(),
            tenant: Some(TenantId::parse("tenant").unwrap()),
            groups: BTreeSet::from([GroupId::parse("flight").unwrap()]),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::from([RoleId::parse("operator").unwrap()]),
            scopes: BTreeSet::<ScopeName>::new(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::<PrincipalAssurance>::new(),
            authenticated_at: None,
        }
    }

    #[test]
    fn highest_matching_membership_wins() {
        let context = WorkContextDefinition {
            id: WorkContextId::parse("mission").unwrap(),
            tenant: TenantId::parse("tenant").unwrap(),
            title: "Mission".into(),
            policy_revision: PolicyVersion::parse("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Group(GroupId::parse("flight").unwrap()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            memberships: vec![
                WorkContextMembershipRule {
                    level: WorkContextMembershipLevel::Viewer,
                    principals: BTreeSet::new(),
                    groups: BTreeSet::from([GroupId::parse("flight").unwrap()]),
                    roles: BTreeSet::new(),
                    oauth_clients: BTreeSet::new(),
                },
                WorkContextMembershipRule {
                    level: WorkContextMembershipLevel::Custodian,
                    principals: BTreeSet::new(),
                    groups: BTreeSet::new(),
                    roles: BTreeSet::from([RoleId::parse("operator").unwrap()]),
                    oauth_clients: BTreeSet::new(),
                },
            ],
        };
        assert_eq!(
            context.membership_for(&principal(), &OAuthClientId::parse("console").unwrap()),
            Some(WorkContextMembershipLevel::Custodian)
        );
    }
    #[test]
    fn configuration_policy_maps_to_unchanged_signed_authority_bytes() {
        let wire = serde_json::json!({"owner":{"kind":"group","id":"operations"},"initialGrants":[],"dataLabels":["cui"]});
        let config: OutputPolicyConfig = serde_json::from_value(wire.clone()).unwrap();
        let policy: WorkContextOutputPolicy = config.into();
        assert_eq!(serde_json::to_value(&policy).unwrap(), serde_json::json!({"owner":{"kind":"group","id":"operations"},"data_labels":["cui"]}));
        let encoded = serde_json::to_value(OutputPolicyConfig::from(policy)).unwrap();
        assert_eq!(encoded, serde_json::json!({"owner":{"kind":"group","id":"operations"},"dataLabels":["cui"]}));
        for key in ["initial_grants", "data_labels"] {
            let mut obsolete = wire.clone();
            obsolete[key] = serde_json::json!([]);
            assert!(serde_json::from_value::<OutputPolicyConfig>(obsolete).is_err());
        }
    }

}
