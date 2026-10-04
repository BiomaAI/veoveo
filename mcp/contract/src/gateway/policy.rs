use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_types::ResourceTemplateUri;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PolicySet {
    pub version: PolicyVersion,
    pub rules: Vec<PolicyRule>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyRule {
    pub id: PolicyRuleId,
    pub effect: PolicyEffect,
    #[schemars(with = "BTreeSet<super::catalog_schema::RegisteredActionSchema>")]
    pub actions: BTreeSet<veoveo_types::ActionName>,
    #[serde(default)]
    pub profiles: BTreeSet<GatewayProfileId>,
    #[serde(default)]
    pub protected_resources: BTreeSet<ProtectedResourceId>,
    #[serde(default)]
    pub servers: BTreeSet<ServerSlug>,
    #[serde(default)]
    pub tools: BTreeSet<LocalToolName>,
    #[serde(default)]
    pub resource_schemes: BTreeSet<ResourceScheme>,
    #[serde(default)]
    pub prompts: BTreeSet<PromptName>,
    #[serde(default)]
    pub principal_ids: BTreeSet<PrincipalId>,
    #[serde(default)]
    pub tenant_ids: BTreeSet<TenantId>,
    #[serde(default)]
    pub groups: BTreeSet<GroupId>,
    #[serde(default)]
    pub roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub required_scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub required_data_labels: BTreeSet<DataLabelId>,
    #[serde(default)]
    pub required_assurances: BTreeSet<PrincipalAssurance>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Principal {
    pub id: PrincipalId,
    pub kind: PrincipalKind,
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default)]
    pub groups: BTreeSet<GroupId>,
    /// Per-group role: the `(GroupId, GroupRole)` pairing that lets a principal
    /// hold different levels in different groups (write in Eng, read in Ops).
    /// A member listed in `groups` without an entry here is treated as `Read`
    /// in that group (see [`Principal::group_memberships`]).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub group_roles: BTreeSet<crate::access::GroupMembership>,
    #[serde(default)]
    pub roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default)]
    pub assurances: BTreeSet<PrincipalAssurance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authenticated_at: Option<DateTime<Utc>>,
}

impl Principal {
    /// The effective `(GroupId, GroupRole)` membership set for access decisions.
    ///
    /// Every group in `groups` yields a membership: its explicit role from
    /// `group_roles` when present, otherwise `Read` (bare membership grants
    /// read-level group access). This is what a [`crate::PlaneCaller`] carries
    /// so the artifact plane can resolve group grants.
    pub fn group_memberships(&self) -> BTreeSet<crate::access::GroupMembership> {
        use crate::access::{GroupMembership, GroupRole};
        let explicit: BTreeMap<_, _> = self
            .group_roles
            .iter()
            .map(|m| (m.group.clone(), m.role))
            .collect();
        self.groups
            .iter()
            .map(|group| GroupMembership {
                group: group.clone(),
                role: explicit.get(group).copied().unwrap_or(GroupRole::Read),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    User,
    Service,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalAssurance {
    UsPerson,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrincipalAuditAttributes {
    pub kind: PrincipalKind,
    #[serde(default)]
    pub groups: BTreeSet<GroupId>,
    #[serde(default)]
    pub roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default)]
    pub assurances: BTreeSet<PrincipalAssurance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authenticated_at: Option<DateTime<Utc>>,
}

impl From<&Principal> for PrincipalAuditAttributes {
    fn from(principal: &Principal) -> Self {
        Self {
            kind: principal.kind,
            groups: principal.groups.clone(),
            roles: principal.roles.clone(),
            scopes: principal.scopes.clone(),
            data_labels: principal.data_labels.clone(),
            assurances: principal.assurances.clone(),
            authenticated_at: principal.authenticated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccessTokenSubject {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_execution: Option<veoveo_audit_contract::AuditManagedExecution>,
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
    pub oauth_client_id: OAuthClientId,
    /// Durable browser session binding. Absence cannot authorize session-bound renewal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_family: Option<GatewayRefreshFamilyId>,
    pub audience: ProtectedResourceId,
    pub work_context: WorkContextId,
    pub invocation_mode: veoveo_types::InvocationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiator: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_id: Option<DelegationId>,
    #[serde(default)]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwt_id: Option<JwtId>,
    pub issued_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyDecision {
    pub effect: PolicyEffect,
    pub reason: PolicyReasonCode,
    pub evaluated_at: DateTime<Utc>,
    pub profile: GatewayProfileId,
    #[schemars(with = "super::catalog_schema::RegisteredActionSchema")]
    pub action: veoveo_types::ActionName,
    pub target: PolicyTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_version: Option<PolicyVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<PolicyRuleId>,
    pub trace_id: TraceId,
}

impl PolicyDecision {
    pub fn deny(
        profile: GatewayProfileId,
        action: impl Into<veoveo_types::ActionName>,
        target: PolicyTarget,
        reason: PolicyReasonCode,
        trace_id: TraceId,
    ) -> Self {
        Self {
            effect: PolicyEffect::Deny,
            reason,
            evaluated_at: Utc::now(),
            profile,
            action: action.into(),
            target,
            principal: None,
            tenant: None,
            policy_version: None,
            rule_id: None,
            trace_id,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyReasonCode {
    PolicyAllow,
    PolicyDeny,
    UnknownProfile,
    UnknownServer,
    UnknownTool,
    UnknownResource,
    UnknownPrompt,
    UnknownTask,
    UnknownArtifact,
    UnknownPrincipal,
    UnknownScope,
    UnknownDataLabel,
    UnknownTenant,
    UnknownTokenIssuer,
    MissingPrincipal,
    MissingTenant,
    MissingGroup,
    MissingRole,
    MissingScope,
    MissingDataLabel,
    MissingPrincipalAssurance,
    TokenAudienceMismatch,
    TokenExpired,
    TokenNotYetValid,
    ReplayDetected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PolicyTarget {
    Gateway,
    Server {
        server: ServerSlug,
    },
    Tool {
        server: ServerSlug,
        tool: LocalToolName,
    },
    Resource {
        server: ServerSlug,
        uri: ResourceUri,
    },
    /// A declaration target for template discovery or argument completion.
    /// ```compile_fail
    /// use veoveo_mcp_contract::{PolicyTarget, ServerSlug};
    /// use veoveo_types::ResourceUri;
    /// PolicyTarget::ResourceTemplate {
    ///     server: ServerSlug::parse("example").unwrap(),
    ///     uri: ResourceUri::new("example://items/literal").unwrap(),
    /// };
    /// ```
    ResourceTemplate {
        server: ServerSlug,
        uri: ResourceTemplateUri,
    },
    Prompt {
        server: ServerSlug,
        prompt: PromptName,
    },
    Task {
        server: ServerSlug,
        task_id: CanonicalTaskId,
    },
    /// A native platform Task addressed by the administration API.
    /// MCP forwarding uses `Task` and its opaque gateway route identity.
    PlatformTask {
        server: ServerSlug,
        task_id: veoveo_types::TaskId,
    },
    Artifact {
        server: ServerSlug,
        artifact_uri: ResourceUri,
    },
    Usage {
        server: ServerSlug,
        usage_uri: ResourceUri,
    },
    #[serde(untagged, skip_deserializing)]
    #[schemars(skip)]
    Owner(veoveo_gateway_contract::AdmittedPolicyTarget),
    #[serde(untagged)]
    #[schemars(skip)]
    Unadmitted(DecodedPolicyTarget),
}

pub use veoveo_types::{AuthMethod, AuthOutcome, AuthReasonCode};

#[cfg(test)]
mod group_membership_tests {
    use super::*;
    use crate::access::{GroupMembership, GroupRole};

    fn principal_with(groups: &[&str], group_roles: &[(&str, GroupRole)]) -> Principal {
        Principal {
            id: PrincipalId::parse("https://idp#u1").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("https://idp").unwrap(),
            subject: TokenSubject::parse("u1").unwrap(),
            tenant: None,
            groups: groups.iter().map(|g| GroupId::parse(*g).unwrap()).collect(),
            group_roles: group_roles
                .iter()
                .map(|(g, r)| GroupMembership {
                    group: GroupId::parse(*g).unwrap(),
                    role: *r,
                })
                .collect(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        }
    }

    #[test]
    fn bare_membership_defaults_to_read() {
        let p = principal_with(&["eng", "ops"], &[]);
        let m = p.group_memberships();
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|gm| gm.role == GroupRole::Read));
    }

    #[test]
    fn explicit_role_overrides_default_per_group() {
        let p = principal_with(&["eng", "ops"], &[("eng", GroupRole::Write)]);
        let m = p.group_memberships();
        let eng = m
            .iter()
            .find(|gm| gm.group == GroupId::parse("eng").unwrap())
            .unwrap();
        let ops = m
            .iter()
            .find(|gm| gm.group == GroupId::parse("ops").unwrap())
            .unwrap();
        assert_eq!(eng.role, GroupRole::Write);
        assert_eq!(ops.role, GroupRole::Read);
    }

    #[test]
    fn role_without_membership_is_ignored() {
        // group_roles for a group the principal is not a member of yields nothing.
        let p = principal_with(&["eng"], &[("secret", GroupRole::Admin)]);
        let m = p.group_memberships();
        assert_eq!(m.len(), 1);
        assert_eq!(
            m.iter().next().unwrap().group,
            GroupId::parse("eng").unwrap()
        );
    }
}

/// A decoded owner target cannot authorize execution before registry admission.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodedPolicyTarget(serde_json::Value);
impl std::fmt::Debug for DecodedPolicyTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DecodedPolicyTarget([REDACTED])")
    }
}
impl Serialize for DecodedPolicyTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for DecodedPolicyTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = veoveo_types::UniqueJsonValue::deserialize(deserializer)?.0;
        if !value.is_object()
            || value
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .is_none()
        {
            return Err(serde::de::Error::custom(
                "policy target requires an object with a kind",
            ));
        }
        Ok(Self(value))
    }
}
impl PolicyTarget {
    pub fn admit(
        self,
        registry: &veoveo_gateway_contract::CatalogRegistry,
    ) -> Result<Self, veoveo_types::ExtensionError> {
        match self {
            Self::Unadmitted(target) => registry.admit_target(target.0).map(Self::Owner),
            Self::Owner(target) => {
                registry.check_target(&target)?;
                Ok(Self::Owner(target))
            }
            kernel => Ok(kernel),
        }
    }
}

impl PolicyTarget {
    pub fn kind(&self) -> &str {
        match self {
            Self::Gateway => "gateway",
            Self::Server { .. } => "server",
            Self::Tool { .. } => "tool",
            Self::Resource { .. } => "resource",
            Self::ResourceTemplate { .. } => "resource_template",
            Self::Prompt { .. } => "prompt",
            Self::Task { .. } => "task",
            Self::PlatformTask { .. } => "platform_task",
            Self::Artifact { .. } => "artifact",
            Self::Usage { .. } => "usage",
            Self::Owner(target) => target.kind().as_str(),
            Self::Unadmitted(_) => "unadmitted",
        }
    }
    pub fn server(&self) -> Option<&ServerSlug> {
        match self {
            Self::Server { server }
            | Self::Tool { server, .. }
            | Self::Resource { server, .. }
            | Self::ResourceTemplate { server, .. }
            | Self::Prompt { server, .. }
            | Self::Task { server, .. }
            | Self::PlatformTask { server, .. }
            | Self::Artifact { server, .. }
            | Self::Usage { server, .. } => Some(server),
            _ => None,
        }
    }
}
