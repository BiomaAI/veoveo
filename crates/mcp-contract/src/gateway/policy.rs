use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub actions: BTreeSet<GatewayAction>,
    #[serde(default)]
    pub profiles: BTreeSet<GatewayProfileId>,
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatewayAction {
    ToolsList,
    ToolsCall,
    ResourcesList,
    ResourcesTemplatesList,
    ResourcesRead,
    ResourcesSubscribe,
    ResourcesUnsubscribe,
    PromptsList,
    PromptsGet,
    CompletionComplete,
    TasksGet,
    TasksResult,
    TasksCancel,
    ArtifactRead,
    UsageRead,
    AdminRead,
    AdminWrite,
}

impl GatewayAction {
    pub fn mcp_method(self) -> Option<&'static str> {
        match self {
            Self::ToolsList => Some("tools/list"),
            Self::ToolsCall => Some("tools/call"),
            Self::ResourcesList => Some("resources/list"),
            Self::ResourcesTemplatesList => Some("resources/templates/list"),
            Self::ResourcesRead => Some("resources/read"),
            Self::ResourcesSubscribe => Some("resources/subscribe"),
            Self::ResourcesUnsubscribe => Some("resources/unsubscribe"),
            Self::PromptsList => Some("prompts/list"),
            Self::PromptsGet => Some("prompts/get"),
            Self::CompletionComplete => Some("completion/complete"),
            Self::TasksGet => Some("tasks/get"),
            Self::TasksResult => Some("tasks/result"),
            Self::TasksCancel => Some("tasks/cancel"),
            Self::ArtifactRead | Self::UsageRead | Self::AdminRead | Self::AdminWrite => None,
        }
    }
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
pub struct AccessTokenSubject {
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
    pub oauth_client_id: OAuthClientId,
    pub audience: ProtectedResourceId,
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
    pub action: GatewayAction,
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
        action: GatewayAction,
        target: PolicyTarget,
        reason: PolicyReasonCode,
        trace_id: TraceId,
    ) -> Self {
        Self {
            effect: PolicyEffect::Deny,
            reason,
            evaluated_at: Utc::now(),
            profile,
            action,
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
    Prompt {
        server: ServerSlug,
        prompt: PromptName,
    },
    Task {
        server: ServerSlug,
        task_id: CanonicalTaskId,
    },
    Artifact {
        server: ServerSlug,
        artifact_uri: ResourceUri,
    },
    Usage {
        server: ServerSlug,
        usage_uri: ResourceUri,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditEvent {
    pub event_id: TraceId,
    pub timestamp: DateTime<Utc>,
    pub trace_id: TraceId,
    pub profile: GatewayProfileId,
    pub method: McpMethodName,
    pub action: GatewayAction,
    pub target: PolicyTarget,
    pub decision: PolicyDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_attributes: Option<PrincipalAuditAttributes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_issuer: Option<TokenIssuer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthAuditEvent {
    pub event_id: TraceId,
    pub timestamp: DateTime<Utc>,
    pub trace_id: TraceId,
    pub profile: GatewayProfileId,
    pub protected_resource: ProtectedResourceId,
    pub outcome: AuthOutcome,
    pub reason: AuthReasonCode,
    pub method: AuthMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_attributes: Option<PrincipalAuditAttributes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_issuer: Option<TokenIssuer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_subject: Option<TokenSubject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwt_id: Option<JwtId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthOutcome {
    Allow,
    Deny,
}

impl AuthOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    BearerJwt,
    OidcAuthorizationCodePkce,
    RefreshToken,
    ClientCredentialsPrivateKeyJwt,
    EnterpriseManagedIdJag,
}

impl AuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BearerJwt => "bearer_jwt",
            Self::OidcAuthorizationCodePkce => "oidc_authorization_code_pkce",
            Self::RefreshToken => "refresh_token",
            Self::ClientCredentialsPrivateKeyJwt => "client_credentials_private_key_jwt",
            Self::EnterpriseManagedIdJag => "enterprise_managed_id_jag",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AuthReasonCode {
    AuthAllow,
    MissingAuthorizationHeader,
    InvalidAuthorizationHeader,
    UnknownIdentityProvider,
    UnknownAuthorizationServer,
    IdentityProviderUnavailable,
    AuthorizationServerUnavailable,
    InvalidAuthConfig,
    InvalidBearerToken,
    InvalidAuthorizationRequest,
    InvalidAuthorizationCode,
    InvalidRefreshToken,
    RefreshTokenDuplicateDelivery,
    RefreshTokenRevoked,
    RefreshTokenReplay,
    InvalidPkce,
    InvalidOidcIdToken,
    InvalidClient,
    UnsupportedGrantType,
    InvalidClientAssertion,
    ClientAssertionReplay,
    InvalidIdentityAssertion,
    IdentityAssertionReplay,
    InvalidScope,
    TokenSigningKeyUnavailable,
    TokenRevoked,
    AuthStateUnavailable,
}

impl AuthReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthAllow => "auth_allow",
            Self::MissingAuthorizationHeader => "missing_authorization_header",
            Self::InvalidAuthorizationHeader => "invalid_authorization_header",
            Self::UnknownIdentityProvider => "unknown_identity_provider",
            Self::UnknownAuthorizationServer => "unknown_authorization_server",
            Self::IdentityProviderUnavailable => "identity_provider_unavailable",
            Self::AuthorizationServerUnavailable => "authorization_server_unavailable",
            Self::InvalidAuthConfig => "invalid_auth_config",
            Self::InvalidBearerToken => "invalid_bearer_token",
            Self::InvalidAuthorizationRequest => "invalid_authorization_request",
            Self::InvalidAuthorizationCode => "invalid_authorization_code",
            Self::InvalidRefreshToken => "invalid_refresh_token",
            Self::RefreshTokenDuplicateDelivery => "refresh_token_duplicate_delivery",
            Self::RefreshTokenRevoked => "refresh_token_revoked",
            Self::RefreshTokenReplay => "refresh_token_replay",
            Self::InvalidPkce => "invalid_pkce",
            Self::InvalidOidcIdToken => "invalid_oidc_id_token",
            Self::InvalidClient => "invalid_client",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::InvalidClientAssertion => "invalid_client_assertion",
            Self::ClientAssertionReplay => "client_assertion_replay",
            Self::InvalidIdentityAssertion => "invalid_identity_assertion",
            Self::IdentityAssertionReplay => "identity_assertion_replay",
            Self::InvalidScope => "invalid_scope",
            Self::TokenSigningKeyUnavailable => "token_signing_key_unavailable",
            Self::TokenRevoked => "token_revoked",
            Self::AuthStateUnavailable => "auth_state_unavailable",
        }
    }
}

#[cfg(test)]
mod group_membership_tests {
    use super::*;
    use crate::access::{GroupMembership, GroupRole};

    fn principal_with(groups: &[&str], group_roles: &[(&str, GroupRole)]) -> Principal {
        Principal {
            id: PrincipalId::new("https://idp#u1").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp").unwrap(),
            subject: TokenSubject::new("u1").unwrap(),
            tenant: None,
            groups: groups.iter().map(|g| GroupId::new(*g).unwrap()).collect(),
            group_roles: group_roles
                .iter()
                .map(|(g, r)| GroupMembership {
                    group: GroupId::new(*g).unwrap(),
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
            .find(|gm| gm.group == GroupId::new("eng").unwrap())
            .unwrap();
        let ops = m
            .iter()
            .find(|gm| gm.group == GroupId::new("ops").unwrap())
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
        assert_eq!(m.iter().next().unwrap().group, GroupId::new("eng").unwrap());
    }
}
