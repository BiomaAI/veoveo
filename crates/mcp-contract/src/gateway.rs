use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! typed_id {
    ($name:ident, $validator:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                $validator(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierError {
    value: String,
    rule: &'static str,
}

impl IdentifierError {
    fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid identifier {:?}: {}", self.value, self.rule)
    }
}

impl std::error::Error for IdentifierError {}

typed_id!(
    ServerSlug,
    validate_path_id,
    "Canonical hosted MCP server id used in manifests, profiles, and gateway routes."
);
typed_id!(
    GatewayProfileId,
    validate_path_id,
    "Gateway profile id exposed under `/mcp/{profile}`."
);
typed_id!(
    GatewayToolName,
    validate_gateway_name,
    "Gateway-scoped tool name after server namespace projection."
);
typed_id!(
    LocalToolName,
    validate_gateway_name,
    "Tool name as exposed by one direct MCP server."
);
typed_id!(
    PromptName,
    validate_gateway_name,
    "Prompt name as exposed by one direct MCP server or gateway profile."
);
typed_id!(
    ResourceScheme,
    validate_uri_scheme,
    "Server-owned resource URI scheme, for example `media`."
);
typed_id!(
    ScopeName,
    validate_token_text,
    "OAuth/OIDC scope value. It must not contain whitespace or control characters."
);
typed_id!(
    DataLabelId,
    validate_token_text,
    "Policy data label such as `cui`, `itar`, `pii`, or an IdP-provided clearance label."
);
typed_id!(
    PrincipalId,
    validate_claim_text,
    "Stable authenticated user or service-principal identity."
);
typed_id!(
    TenantId,
    validate_claim_text,
    "Tenant, organization, or customer boundary identifier."
);
typed_id!(
    GroupId,
    validate_claim_text,
    "Identity-provider group identifier used by gateway policy."
);
typed_id!(
    RoleId,
    validate_claim_text,
    "Identity-provider role identifier used by gateway policy."
);
typed_id!(
    PolicyVersion,
    validate_token_text,
    "Immutable policy version identifier emitted with decisions and audit records."
);
typed_id!(
    PolicyRuleId,
    validate_token_text,
    "Policy rule identifier used for decision evidence."
);
typed_id!(
    SecretReferenceId,
    validate_token_text,
    "Reference to a secret managed outside control data."
);
typed_id!(
    ProtectedResourceId,
    validate_claim_text,
    "OAuth protected-resource identifier, usually the gateway profile URL."
);
typed_id!(
    TokenIssuer,
    validate_claim_text,
    "Expected token issuer identifier."
);
typed_id!(
    TokenSubject,
    validate_claim_text,
    "Subject claim from an authenticated access token or identity assertion."
);
typed_id!(
    JwtId,
    validate_claim_text,
    "JWT id used for replay protection or revocation tracking."
);
typed_id!(
    TraceId,
    validate_token_text,
    "Request trace/correlation id used in audit and runtime state."
);
typed_id!(
    GatewayTaskId,
    validate_token_text,
    "Gateway task id visible to MCP clients."
);
typed_id!(
    UpstreamTaskId,
    validate_token_text,
    "Task id owned by one hosted upstream MCP server."
);
typed_id!(
    McpMethodName,
    validate_token_text,
    "MCP JSON-RPC method name used in policy and audit events."
);
typed_id!(
    SecretLocator,
    validate_claim_text,
    "External secret locator. This is a reference path, not a secret value."
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayControlPlane {
    pub servers: Vec<ServerManifest>,
    pub profiles: Vec<GatewayProfile>,
    pub policies: Vec<PolicySet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretReference>,
    #[serde(default)]
    pub metadata: Value,
}

impl GatewayControlPlane {
    pub fn validate(&self) -> Result<(), GatewayControlPlaneError> {
        let mut servers = BTreeSet::new();
        for server in &self.servers {
            if !servers.insert(server.slug.clone()) {
                return Err(GatewayControlPlaneError::DuplicateServer(
                    server.slug.clone(),
                ));
            }
        }

        let mut policies = BTreeSet::new();
        for policy in &self.policies {
            if !policies.insert(policy.version.clone()) {
                return Err(GatewayControlPlaneError::DuplicatePolicy(
                    policy.version.clone(),
                ));
            }
        }

        let mut profiles = BTreeSet::new();
        for profile in &self.profiles {
            if !profiles.insert(profile.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateProfile(
                    profile.id.clone(),
                ));
            }
            if !policies.contains(&profile.policy_version) {
                return Err(GatewayControlPlaneError::UnknownPolicy {
                    profile: profile.id.clone(),
                    policy_version: profile.policy_version.clone(),
                });
            }
            for exposure in &profile.servers {
                if !servers.contains(&exposure.server) {
                    return Err(GatewayControlPlaneError::UnknownServer {
                        profile: profile.id.clone(),
                        server: exposure.server.clone(),
                    });
                }
            }
        }

        let mut secrets = BTreeSet::new();
        for secret in &self.secrets {
            if !secrets.insert(secret.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateSecret(secret.id.clone()));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayControlPlaneError {
    DuplicateServer(ServerSlug),
    DuplicateProfile(GatewayProfileId),
    DuplicatePolicy(PolicyVersion),
    DuplicateSecret(SecretReferenceId),
    UnknownServer {
        profile: GatewayProfileId,
        server: ServerSlug,
    },
    UnknownPolicy {
        profile: GatewayProfileId,
        policy_version: PolicyVersion,
    },
}

impl fmt::Display for GatewayControlPlaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateServer(server) => write!(f, "duplicate server manifest `{server}`"),
            Self::DuplicateProfile(profile) => write!(f, "duplicate gateway profile `{profile}`"),
            Self::DuplicatePolicy(policy) => write!(f, "duplicate policy version `{policy}`"),
            Self::DuplicateSecret(secret) => write!(f, "duplicate secret reference `{secret}`"),
            Self::UnknownServer { profile, server } => write!(
                f,
                "gateway profile `{profile}` references unknown server `{server}`"
            ),
            Self::UnknownPolicy {
                profile,
                policy_version,
            } => write!(
                f,
                "gateway profile `{profile}` references unknown policy `{policy_version}`"
            ),
        }
    }
}

impl std::error::Error for GatewayControlPlaneError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServerManifest {
    pub slug: ServerSlug,
    pub uri_scheme: ResourceScheme,
    pub mount_path: MountPath,
    pub mcp_path: MountPath,
    pub upstream: UpstreamEndpoint,
    pub capabilities: McpSurfaceCapabilities,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<LocalToolName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<PromptName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_scopes: Vec<ScopeName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_routes: Vec<OwnedRoute>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayProfile {
    pub id: GatewayProfileId,
    pub protected_resource: ProtectedResourceId,
    pub policy_version: PolicyVersion,
    pub auth_modes: BTreeSet<AuthMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_scopes: Vec<ScopeName>,
    pub servers: Vec<ProfileServerExposure>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileServerExposure {
    pub server: ServerSlug,
    pub tools: Exposure<LocalToolName>,
    pub resources: Exposure<ResourceSelector>,
    pub prompts: Exposure<PromptName>,
    pub completions: CompletionExposure,
    pub tasks: TaskExposure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "mode", content = "items")]
pub enum Exposure<T> {
    All,
    Listed(Vec<T>),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ResourceSelector {
    Scheme { scheme: ResourceScheme },
    UriPrefix { prefix: ResourceUriPrefix },
    Template { uri_template: ResourceUriTemplate },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompletionExposure {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskExposure {
    Enabled,
    Disabled,
}

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
    #[serde(default)]
    pub roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authenticated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    User,
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AccessTokenSubject {
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    UnknownTokenIssuer,
    MissingScope,
    MissingDataLabel,
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
        gateway_task_id: GatewayTaskId,
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
    pub tenant: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_issuer: Option<TokenIssuer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayTaskMapping {
    pub gateway_task_id: GatewayTaskId,
    pub upstream_server: ServerSlug,
    pub upstream_task_id: UpstreamTaskId,
    pub profile: GatewayProfileId,
    pub owner: PrincipalId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretReference {
    pub id: SecretReferenceId,
    pub source: SecretSource,
    pub purpose: SecretPurpose,
    pub locator: SecretLocator,
    pub owner: SecretOwner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_hint: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretSource {
    Env,
    Vault,
    HcpVault,
    CloudSecretManager,
    KmsBackedStore,
    EnterpriseManaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretPurpose {
    ProviderApiKey,
    WebhookSecret,
    OAuthClientSecret,
    GatewaySigningKey,
    JwksPrivateKey,
    TokenExchangeCredential,
    ObjectStoreCredential,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SecretOwner {
    Gateway,
    Profile { profile: GatewayProfileId },
    Server { server: ServerSlug },
    Tenant { tenant: TenantId },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MountPath(String);

impl MountPath {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_mount_path(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for MountPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for MountPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for MountPath {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<MountPath> for String {
    fn from(value: MountPath) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUri(String);

impl ResourceUri {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_resource_uri(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ResourceUri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ResourceUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ResourceUri {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUri> for String {
    fn from(value: ResourceUri) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUriPrefix(String);

impl ResourceUriPrefix {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_resource_uri(&value)?;
        Ok(Self(value))
    }
}

impl TryFrom<String> for ResourceUriPrefix {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUriPrefix> for String {
    fn from(value: ResourceUriPrefix) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUriTemplate(String);

impl ResourceUriTemplate {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_uri_template(&value)?;
        Ok(Self(value))
    }
}

impl TryFrom<String> for ResourceUriTemplate {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUriTemplate> for String {
    fn from(value: ResourceUriTemplate) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpstreamEndpoint {
    pub transport: UpstreamTransport,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamTransport {
    StreamableHttp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OwnedRoute {
    pub path: MountPath,
    pub purpose: OwnedRoutePurpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OwnedRoutePurpose {
    Webhook,
    ArtifactBytes,
    ProviderFetchableFiles,
    Health,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpSurfaceCapabilities {
    pub tools: bool,
    pub resources: bool,
    pub resource_templates: bool,
    pub resource_subscriptions: bool,
    pub prompts: bool,
    pub completions: bool,
    pub tasks: bool,
    pub notifications: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    OidcAuthorizationCodePkce,
    EnterpriseManagedAuthorization,
    OAuthClientCredentials,
}

fn validate_path_id(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(
            value,
            "must not be empty and must contain lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

fn validate_gateway_name(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

fn validate_uri_scheme(value: &str) -> Result<(), IdentifierError> {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(IdentifierError::new(value, "must not be empty"));
    };
    if !first.is_ascii_lowercase() {
        return Err(IdentifierError::new(
            value,
            "must start with a lowercase ASCII letter",
        ));
    }
    if !bytes.all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'+' || b == b'-' || b == b'.'
    }) {
        return Err(IdentifierError::new(
            value,
            "must follow URI scheme syntax with lowercase ASCII characters",
        ));
    }
    Ok(())
}

fn validate_token_text(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    Ok(())
}

fn validate_claim_text(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(char::is_control) {
        return Err(IdentifierError::new(
            value,
            "must not contain control characters",
        ));
    }
    Ok(())
}

fn validate_mount_path(value: &str) -> Result<(), IdentifierError> {
    if !value.starts_with('/') || value.len() == 1 {
        return Err(IdentifierError::new(
            value,
            "must be an absolute path with at least one segment",
        ));
    }
    if value.ends_with('/') {
        return Err(IdentifierError::new(value, "must not end with slash"));
    }
    if value.contains("//") || value.contains(['?', '#']) {
        return Err(IdentifierError::new(
            value,
            "must not contain empty segments, query, or fragment",
        ));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    Ok(())
}

fn validate_resource_uri(value: &str) -> Result<(), IdentifierError> {
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err(IdentifierError::new(
            value,
            "must be an absolute server-owned resource URI",
        ));
    };
    validate_uri_scheme(scheme)?;
    if rest.is_empty() || rest.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must include a non-empty path and no whitespace/control characters",
        ));
    }
    Ok(())
}

fn validate_uri_template(value: &str) -> Result<(), IdentifierError> {
    validate_resource_uri(value)?;
    if !value.contains('{') || !value.contains('}') {
        return Err(IdentifierError::new(
            value,
            "must include at least one URI-template variable",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media_manifest() -> ServerManifest {
        ServerManifest {
            slug: ServerSlug::new("media").unwrap(),
            uri_scheme: ResourceScheme::new("media").unwrap(),
            mount_path: MountPath::new("/media").unwrap(),
            mcp_path: MountPath::new("/media/mcp").unwrap(),
            upstream: UpstreamEndpoint {
                transport: UpstreamTransport::StreamableHttp,
                url: "http://media-mcp:8787/media/mcp".to_string(),
            },
            capabilities: McpSurfaceCapabilities {
                tools: true,
                resources: true,
                resource_templates: true,
                resource_subscriptions: true,
                prompts: true,
                completions: true,
                tasks: true,
                notifications: true,
            },
            tools: vec![LocalToolName::new("run").unwrap()],
            prompts: vec![PromptName::new("model_help").unwrap()],
            required_scopes: vec![ScopeName::new("media:use").unwrap()],
            owned_routes: vec![OwnedRoute {
                path: MountPath::new("/media/webhooks").unwrap(),
                purpose: OwnedRoutePurpose::Webhook,
            }],
            metadata: Value::Null,
        }
    }

    fn default_policy() -> PolicySet {
        PolicySet {
            version: PolicyVersion::new("2026-07-02").unwrap(),
            rules: vec![PolicyRule {
                id: PolicyRuleId::new("allow_media_use").unwrap(),
                effect: PolicyEffect::Allow,
                actions: BTreeSet::from([GatewayAction::ToolsCall]),
                profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
                servers: BTreeSet::from([ServerSlug::new("media").unwrap()]),
                tools: BTreeSet::from([LocalToolName::new("run").unwrap()]),
                resource_schemes: BTreeSet::new(),
                prompts: BTreeSet::new(),
                principal_ids: BTreeSet::new(),
                tenant_ids: BTreeSet::new(),
                groups: BTreeSet::new(),
                roles: BTreeSet::new(),
                required_scopes: BTreeSet::from([ScopeName::new("media:use").unwrap()]),
                required_data_labels: BTreeSet::new(),
                metadata: Value::Null,
            }],
            metadata: Value::Null,
        }
    }

    fn default_profile() -> GatewayProfile {
        GatewayProfile {
            id: GatewayProfileId::new("default").unwrap(),
            protected_resource: ProtectedResourceId::new("https://veoveo.bioma.ai/mcp/default")
                .unwrap(),
            policy_version: PolicyVersion::new("2026-07-02").unwrap(),
            auth_modes: BTreeSet::from([
                AuthMode::EnterpriseManagedAuthorization,
                AuthMode::OAuthClientCredentials,
                AuthMode::OidcAuthorizationCodePkce,
            ]),
            required_scopes: vec![ScopeName::new("media:use").unwrap()],
            servers: vec![ProfileServerExposure {
                server: ServerSlug::new("media").unwrap(),
                tools: Exposure::Listed(vec![LocalToolName::new("run").unwrap()]),
                resources: Exposure::Listed(vec![ResourceSelector::Scheme {
                    scheme: ResourceScheme::new("media").unwrap(),
                }]),
                prompts: Exposure::All,
                completions: CompletionExposure::Enabled,
                tasks: TaskExposure::Enabled,
            }],
            metadata: Value::Null,
        }
    }

    #[test]
    fn identifiers_reject_invalid_wire_values() {
        assert!(ServerSlug::new("Media").is_err());
        assert!(GatewayProfileId::new("default/profile").is_err());
        assert!(ResourceScheme::new("1media").is_err());
        assert!(MountPath::new("media").is_err());
        assert!(ResourceUri::new("media://artifact/abc").is_ok());
        assert!(ResourceUriTemplate::new("media://model").is_err());
    }

    #[test]
    fn control_plane_validates_cross_references() {
        let config = GatewayControlPlane {
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            secrets: vec![SecretReference {
                id: SecretReferenceId::new("media_provider_key").unwrap(),
                source: SecretSource::Env,
                purpose: SecretPurpose::ProviderApiKey,
                locator: SecretLocator::new("MEDIA_PROVIDER_API_KEY").unwrap(),
                owner: SecretOwner::Server {
                    server: ServerSlug::new("media").unwrap(),
                },
                rotation_hint: None,
                metadata: Value::Null,
            }],
            metadata: Value::Null,
        };

        config.validate().expect("valid gateway control plane");
    }

    #[test]
    fn control_plane_rejects_unknown_server_reference() {
        let mut profile = default_profile();
        profile.servers[0].server = ServerSlug::new("simulation").unwrap();
        let config = GatewayControlPlane {
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            secrets: vec![],
            metadata: Value::Null,
        };

        let err = config.validate().expect_err("unknown server must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownServer { .. }
        ));
    }

    #[test]
    fn policy_decision_defaults_to_explicit_deny() {
        let decision = PolicyDecision::deny(
            GatewayProfileId::new("default").unwrap(),
            GatewayAction::ToolsCall,
            PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            PolicyReasonCode::MissingScope,
            TraceId::new("trace-1").unwrap(),
        );

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingScope);
    }
}
