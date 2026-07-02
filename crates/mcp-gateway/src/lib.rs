use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::Path,
    sync::Arc,
};

pub mod auth;

use anyhow::{Context, Result};
pub use auth::{AuthError, AuthenticatedSubject, BearerToken, JwtAuthConfig, JwtVerifier};
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    GatewayAction, GatewayControlPlane, GatewayProfile, GatewayProfileId, GatewayToolName,
    IdentityProvider, IdentityProviderId, LocalToolName, McpMethodName, PolicyDecision,
    PolicyEffect, PolicyReasonCode, PolicyRule, PolicySet, PolicyTarget, PolicyVersion, Principal,
    ResourceScheme, ScopeName, ServerManifest, ServerSlug, TraceId,
};

#[derive(Debug, Clone)]
pub struct GatewayCatalog {
    control_plane: Arc<GatewayControlPlane>,
    identity_providers: BTreeMap<IdentityProviderId, usize>,
    servers: BTreeMap<ServerSlug, usize>,
    profiles: BTreeMap<GatewayProfileId, usize>,
    policies: BTreeMap<PolicyVersion, usize>,
}

impl GatewayCatalog {
    pub fn from_control_plane(control_plane: GatewayControlPlane) -> Result<Self> {
        control_plane.validate()?;

        let identity_providers = control_plane
            .identity_providers
            .iter()
            .enumerate()
            .map(|(index, identity_provider)| (identity_provider.id.clone(), index))
            .collect();
        let servers = control_plane
            .servers
            .iter()
            .enumerate()
            .map(|(index, server)| (server.slug.clone(), index))
            .collect();
        let profiles = control_plane
            .profiles
            .iter()
            .enumerate()
            .map(|(index, profile)| (profile.id.clone(), index))
            .collect();
        let policies = control_plane
            .policies
            .iter()
            .enumerate()
            .map(|(index, policy)| (policy.version.clone(), index))
            .collect();

        Ok(Self {
            control_plane: Arc::new(control_plane),
            identity_providers,
            servers,
            profiles,
            policies,
        })
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read gateway control plane {}", path.display()))?;
        let control_plane: GatewayControlPlane = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse gateway control plane {}", path.display()))?;
        Self::from_control_plane(control_plane)
            .with_context(|| format!("invalid gateway control plane {}", path.display()))
    }

    pub fn control_plane(&self) -> &GatewayControlPlane {
        &self.control_plane
    }

    pub fn server_count(&self) -> usize {
        self.control_plane.servers.len()
    }

    pub fn profile_count(&self) -> usize {
        self.control_plane.profiles.len()
    }

    pub fn profile(&self, profile_id: &GatewayProfileId) -> Option<&GatewayProfile> {
        self.profiles
            .get(profile_id)
            .map(|index| &self.control_plane.profiles[*index])
    }

    pub fn identity_provider(
        &self,
        identity_provider_id: &IdentityProviderId,
    ) -> Option<&IdentityProvider> {
        self.identity_providers
            .get(identity_provider_id)
            .map(|index| &self.control_plane.identity_providers[*index])
    }

    pub fn protected_resource_metadata(
        &self,
        profile_id: &GatewayProfileId,
    ) -> Result<ProtectedResourceMetadata, GatewayMetadataError> {
        let profile = self
            .profile(profile_id)
            .ok_or_else(|| GatewayMetadataError::UnknownProfile(profile_id.clone()))?;
        let identity_provider = self
            .identity_provider(&profile.identity_provider)
            .ok_or_else(|| GatewayMetadataError::UnknownIdentityProvider {
                profile: profile.id.clone(),
                identity_provider: profile.identity_provider.clone(),
            })?;

        Ok(ProtectedResourceMetadata {
            resource: profile.protected_resource.to_string(),
            authorization_servers: vec![identity_provider.issuer.to_string()],
            scopes_supported: profile
                .required_scopes
                .iter()
                .map(ToString::to_string)
                .collect(),
            bearer_methods_supported: vec!["header".to_string()],
        })
    }

    pub fn server(&self, server_slug: &ServerSlug) -> Option<&ServerManifest> {
        self.servers
            .get(server_slug)
            .map(|index| &self.control_plane.servers[*index])
    }

    pub fn policy(&self, version: &PolicyVersion) -> Option<&PolicySet> {
        self.policies
            .get(version)
            .map(|index| &self.control_plane.policies[*index])
    }

    pub fn project_tool_name(
        &self,
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> Result<GatewayToolName, GatewayNameError> {
        if self.server(server).is_none() {
            return Err(GatewayNameError::UnknownServer(server.clone()));
        }
        GatewayToolProjection::new(server.clone(), tool.clone()).gateway_name()
    }

    pub fn parse_tool_name(
        &self,
        name: &GatewayToolName,
    ) -> Result<GatewayToolProjection, GatewayNameError> {
        let projection = GatewayToolProjection::parse(name)?;
        if self.server(&projection.server).is_none() {
            return Err(GatewayNameError::UnknownServer(projection.server));
        }
        Ok(projection)
    }

    pub fn decide(&self, request: PolicyRequest<'_>) -> PolicyDecision {
        let Some(profile) = self.profile(request.profile) else {
            return deny(
                &request,
                PolicyReasonCode::UnknownProfile,
                PolicyTarget::Gateway,
                None,
            );
        };

        let Some(policy) = self.policy(&profile.policy_version) else {
            return deny(
                &request,
                PolicyReasonCode::PolicyDeny,
                request.target.clone(),
                None,
            );
        };

        if let Err(reason) = self.profile_allows_target(profile, request.action, request.target) {
            return deny(
                &request,
                reason,
                request.target.clone(),
                Some(policy.version.clone()),
            );
        }

        if !has_required_scopes(&request.principal.scopes, &profile.required_scopes) {
            return deny(
                &request,
                PolicyReasonCode::MissingScope,
                request.target.clone(),
                Some(policy.version.clone()),
            );
        }

        let matching_denial = policy
            .rules
            .iter()
            .find(|rule| rule.effect == PolicyEffect::Deny && rule_matches(rule, profile, &request))
            .map(|rule| rule.id.clone());
        if let Some(rule_id) = matching_denial {
            return decision(
                &request,
                PolicyEffect::Deny,
                PolicyReasonCode::PolicyDeny,
                request.target.clone(),
                Some(policy.version.clone()),
                Some(rule_id),
            );
        }

        let matching_allow = policy
            .rules
            .iter()
            .find(|rule| {
                rule.effect == PolicyEffect::Allow && rule_matches(rule, profile, &request)
            })
            .map(|rule| rule.id.clone());
        if let Some(rule_id) = matching_allow {
            return decision(
                &request,
                PolicyEffect::Allow,
                PolicyReasonCode::PolicyAllow,
                request.target.clone(),
                Some(policy.version.clone()),
                Some(rule_id),
            );
        }

        deny(
            &request,
            PolicyReasonCode::PolicyDeny,
            request.target.clone(),
            Some(policy.version.clone()),
        )
    }

    fn profile_allows_target(
        &self,
        profile: &GatewayProfile,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> Result<(), PolicyReasonCode> {
        match target {
            PolicyTarget::Gateway => Ok(()),
            PolicyTarget::Server { server } => {
                if self.server(server).is_none() {
                    return Err(PolicyReasonCode::UnknownServer);
                }
                profile
                    .servers
                    .iter()
                    .any(|exposure| &exposure.server == server)
                    .then_some(())
                    .ok_or(PolicyReasonCode::PolicyDeny)
            }
            PolicyTarget::Tool { server, tool } => {
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                if !manifest.tools.is_empty() && !manifest.tools.iter().any(|known| known == tool) {
                    return Err(PolicyReasonCode::UnknownTool);
                }
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure_contains(&exposure.tools, tool) {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::Resource { server, uri }
            | PolicyTarget::Artifact {
                server,
                artifact_uri: uri,
            }
            | PolicyTarget::Usage {
                server,
                usage_uri: uri,
            } => {
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                let scheme =
                    resource_scheme(uri.as_str()).ok_or(PolicyReasonCode::UnknownResource)?;
                if scheme != manifest.uri_scheme {
                    return Err(PolicyReasonCode::UnknownResource);
                }
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if matches!(&exposure.resources, veoveo_mcp_contract::Exposure::All)
                    || exposure.resources.iter().any(|selector| match selector {
                        veoveo_mcp_contract::ResourceSelector::Scheme { scheme: allowed } => {
                            allowed == &scheme
                        }
                        veoveo_mcp_contract::ResourceSelector::UriPrefix { prefix } => {
                            uri.as_str().starts_with(prefix.as_ref())
                        }
                        veoveo_mcp_contract::ResourceSelector::Template { .. } => false,
                    })
                {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::Prompt { server, prompt } => {
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                if !manifest.prompts.is_empty() && !manifest.prompts.iter().any(|p| p == prompt) {
                    return Err(PolicyReasonCode::UnknownPrompt);
                }
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure_contains(&exposure.prompts, prompt) {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::Task {
                server,
                gateway_task_id: _,
            } => {
                let _manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled
                    || matches!(
                        action,
                        GatewayAction::ResourcesList | GatewayAction::ResourcesTemplatesList
                    )
                {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
    pub bearer_methods_supported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayMetadataError {
    UnknownProfile(GatewayProfileId),
    UnknownIdentityProvider {
        profile: GatewayProfileId,
        identity_provider: IdentityProviderId,
    },
}

impl fmt::Display for GatewayMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProfile(profile) => write!(f, "unknown gateway profile `{profile}`"),
            Self::UnknownIdentityProvider {
                profile,
                identity_provider,
            } => write!(
                f,
                "gateway profile `{profile}` references unknown identity provider `{identity_provider}`"
            ),
        }
    }
}

impl std::error::Error for GatewayMetadataError {}

pub fn www_authenticate_challenge(metadata_url: &str, scopes: &[ScopeName]) -> String {
    let scope = scopes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    if scope.is_empty() {
        format!(r#"Bearer resource_metadata="{metadata_url}""#)
    } else {
        format!(r#"Bearer resource_metadata="{metadata_url}", scope="{scope}""#)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayToolProjection {
    pub server: ServerSlug,
    pub tool: LocalToolName,
}

impl GatewayToolProjection {
    pub fn new(server: ServerSlug, tool: LocalToolName) -> Self {
        Self { server, tool }
    }

    pub fn gateway_name(&self) -> Result<GatewayToolName, GatewayNameError> {
        GatewayToolName::new(format!("{}__{}", self.server, self.tool))
            .map_err(GatewayNameError::InvalidProjectedToolName)
    }

    pub fn parse(name: &GatewayToolName) -> Result<Self, GatewayNameError> {
        let Some((server, tool)) = name.as_str().split_once("__") else {
            return Err(GatewayNameError::MissingNamespace(name.clone()));
        };
        if tool.contains("__") {
            return Err(GatewayNameError::InvalidNamespaceShape(name.clone()));
        }
        Ok(Self {
            server: ServerSlug::new(server).map_err(GatewayNameError::InvalidServerSlug)?,
            tool: LocalToolName::new(tool).map_err(GatewayNameError::InvalidLocalToolName)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayNameError {
    UnknownServer(ServerSlug),
    MissingNamespace(GatewayToolName),
    InvalidNamespaceShape(GatewayToolName),
    InvalidServerSlug(veoveo_mcp_contract::IdentifierError),
    InvalidLocalToolName(veoveo_mcp_contract::IdentifierError),
    InvalidProjectedToolName(veoveo_mcp_contract::IdentifierError),
}

impl fmt::Display for GatewayNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownServer(server) => write!(f, "unknown server `{server}`"),
            Self::MissingNamespace(name) => {
                write!(f, "gateway tool `{name}` is missing server namespace")
            }
            Self::InvalidNamespaceShape(name) => {
                write!(f, "gateway tool `{name}` has an invalid namespace shape")
            }
            Self::InvalidServerSlug(err)
            | Self::InvalidLocalToolName(err)
            | Self::InvalidProjectedToolName(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for GatewayNameError {}

#[derive(Debug, Clone)]
pub struct PolicyRequest<'a> {
    pub principal: &'a Principal,
    pub profile: &'a GatewayProfileId,
    pub action: GatewayAction,
    pub target: &'a PolicyTarget,
    pub trace_id: &'a TraceId,
}

pub fn mcp_method_name(action: GatewayAction) -> Result<McpMethodName> {
    let Some(method) = action.mcp_method() else {
        anyhow::bail!("gateway action {action:?} does not map to one MCP method")
    };
    Ok(McpMethodName::new(method)?)
}

fn deny(
    request: &PolicyRequest<'_>,
    reason: PolicyReasonCode,
    target: PolicyTarget,
    policy_version: Option<PolicyVersion>,
) -> PolicyDecision {
    decision(
        request,
        PolicyEffect::Deny,
        reason,
        target,
        policy_version,
        None,
    )
}

fn decision(
    request: &PolicyRequest<'_>,
    effect: PolicyEffect,
    reason: PolicyReasonCode,
    target: PolicyTarget,
    policy_version: Option<PolicyVersion>,
    rule_id: Option<veoveo_mcp_contract::PolicyRuleId>,
) -> PolicyDecision {
    PolicyDecision {
        effect,
        reason,
        evaluated_at: chrono::Utc::now(),
        profile: request.profile.clone(),
        action: request.action,
        target,
        principal: Some(request.principal.id.clone()),
        tenant: request.principal.tenant.clone(),
        policy_version,
        rule_id,
        trace_id: request.trace_id.clone(),
    }
}

fn rule_matches(rule: &PolicyRule, profile: &GatewayProfile, request: &PolicyRequest<'_>) -> bool {
    if !rule.actions.contains(&request.action) {
        return false;
    }
    if !rule.profiles.is_empty() && !rule.profiles.contains(&profile.id) {
        return false;
    }
    if !matches_target_filters(rule, request.target) {
        return false;
    }
    if !rule.principal_ids.is_empty() && !rule.principal_ids.contains(&request.principal.id) {
        return false;
    }
    if !rule.tenant_ids.is_empty() {
        let Some(tenant) = &request.principal.tenant else {
            return false;
        };
        if !rule.tenant_ids.contains(tenant) {
            return false;
        }
    }
    if !rule.groups.is_empty() && !intersects(&rule.groups, &request.principal.groups) {
        return false;
    }
    if !rule.roles.is_empty() && !intersects(&rule.roles, &request.principal.roles) {
        return false;
    }
    if !rule.required_scopes.is_empty()
        && !rule.required_scopes.is_subset(&request.principal.scopes)
    {
        return false;
    }
    if !rule.required_data_labels.is_empty()
        && !rule
            .required_data_labels
            .is_subset(&request.principal.data_labels)
    {
        return false;
    }
    true
}

fn matches_target_filters(rule: &PolicyRule, target: &PolicyTarget) -> bool {
    match target {
        PolicyTarget::Gateway => true,
        PolicyTarget::Server { server } => {
            filter_matches(&rule.servers, server)
                && rule.tools.is_empty()
                && rule.resource_schemes.is_empty()
                && rule.prompts.is_empty()
        }
        PolicyTarget::Tool { server, tool } => {
            filter_matches(&rule.servers, server) && filter_matches(&rule.tools, tool)
        }
        PolicyTarget::Resource { server, uri }
        | PolicyTarget::Artifact {
            server,
            artifact_uri: uri,
        }
        | PolicyTarget::Usage {
            server,
            usage_uri: uri,
        } => {
            let Some(scheme) = resource_scheme(uri.as_str()) else {
                return false;
            };
            filter_matches(&rule.servers, server) && filter_matches(&rule.resource_schemes, &scheme)
        }
        PolicyTarget::Prompt { server, prompt } => {
            filter_matches(&rule.servers, server) && filter_matches(&rule.prompts, prompt)
        }
        PolicyTarget::Task {
            server,
            gateway_task_id: _,
        } => filter_matches(&rule.servers, server),
    }
}

fn resource_scheme(uri: &str) -> Option<ResourceScheme> {
    let (scheme, _) = uri.split_once("://")?;
    ResourceScheme::new(scheme).ok()
}

fn has_required_scopes(
    principal_scopes: &BTreeSet<veoveo_mcp_contract::ScopeName>,
    required: &[veoveo_mcp_contract::ScopeName],
) -> bool {
    required
        .iter()
        .all(|scope| principal_scopes.contains(scope))
}

fn exposure_contains<T: PartialEq>(exposure: &veoveo_mcp_contract::Exposure<T>, item: &T) -> bool {
    match exposure {
        veoveo_mcp_contract::Exposure::All => true,
        veoveo_mcp_contract::Exposure::Listed(items) => items.iter().any(|allowed| allowed == item),
        veoveo_mcp_contract::Exposure::None => false,
    }
}

trait ExposureResourceIter {
    fn iter(&self) -> Box<dyn Iterator<Item = &veoveo_mcp_contract::ResourceSelector> + '_>;
}

impl ExposureResourceIter for veoveo_mcp_contract::Exposure<veoveo_mcp_contract::ResourceSelector> {
    fn iter(&self) -> Box<dyn Iterator<Item = &veoveo_mcp_contract::ResourceSelector> + '_> {
        match self {
            veoveo_mcp_contract::Exposure::All | veoveo_mcp_contract::Exposure::None => {
                Box::new([].iter())
            }
            veoveo_mcp_contract::Exposure::Listed(items) => Box::new(items.iter()),
        }
    }
}

fn filter_matches<T: Ord>(filter: &BTreeSet<T>, value: &T) -> bool {
    filter.is_empty() || filter.contains(value)
}

fn intersects<T: Ord>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> bool {
    left.iter().any(|value| right.contains(value))
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use serde_json::Value;
    use veoveo_mcp_contract::{
        AuthMode, CompletionExposure, DataLabelId, Exposure, GatewayControlPlaneError,
        GatewayTaskId, HttpsUrl, IdentityProvider, IdentityProviderId, MountPath, OwnedRoute,
        OwnedRoutePurpose, PrincipalId, PrincipalKind, ProfileServerExposure, ProtectedResourceId,
        ResourceSelector, ScopeName, SecretLocator, SecretOwner, SecretPurpose, SecretReference,
        SecretReferenceId, SecretSource, TaskExposure, TenantId, TokenIssuer, TokenSubject,
        UpstreamEndpoint, UpstreamTransport,
    };

    use super::*;

    fn identity_provider() -> IdentityProvider {
        IdentityProvider {
            id: IdentityProviderId::new("enterprise").unwrap(),
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            jwks_uri: HttpsUrl::new("https://idp.example.com/.well-known/jwks.json").unwrap(),
            authorization_endpoint: Some(
                HttpsUrl::new("https://idp.example.com/oauth2/authorize").unwrap(),
            ),
            token_endpoint: Some(HttpsUrl::new("https://idp.example.com/oauth2/token").unwrap()),
            enterprise_managed_authorization_endpoint: Some(
                HttpsUrl::new("https://idp.example.com/oauth2/id-jag").unwrap(),
            ),
            client_credentials_endpoint: Some(
                HttpsUrl::new("https://idp.example.com/oauth2/token").unwrap(),
            ),
            metadata: Value::Null,
        }
    }

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
            capabilities: veoveo_mcp_contract::McpSurfaceCapabilities {
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
            prompts: vec![],
            required_scopes: vec![ScopeName::new("media:use").unwrap()],
            owned_routes: vec![OwnedRoute {
                path: MountPath::new("/media/webhooks").unwrap(),
                purpose: OwnedRoutePurpose::Webhook,
            }],
            metadata: Value::Null,
        }
    }

    fn policy() -> PolicySet {
        PolicySet {
            version: PolicyVersion::new("2026-07-02").unwrap(),
            rules: vec![PolicyRule {
                id: veoveo_mcp_contract::PolicyRuleId::new("allow_media_run").unwrap(),
                effect: PolicyEffect::Allow,
                actions: BTreeSet::from([GatewayAction::ToolsCall]),
                profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
                servers: BTreeSet::from([ServerSlug::new("media").unwrap()]),
                tools: BTreeSet::from([LocalToolName::new("run").unwrap()]),
                resource_schemes: BTreeSet::new(),
                prompts: BTreeSet::new(),
                principal_ids: BTreeSet::new(),
                tenant_ids: BTreeSet::from([TenantId::new("tenant-a").unwrap()]),
                groups: BTreeSet::new(),
                roles: BTreeSet::new(),
                required_scopes: BTreeSet::from([ScopeName::new("media:use").unwrap()]),
                required_data_labels: BTreeSet::new(),
                metadata: Value::Null,
            }],
            metadata: Value::Null,
        }
    }

    fn profile() -> GatewayProfile {
        GatewayProfile {
            id: GatewayProfileId::new("default").unwrap(),
            identity_provider: IdentityProviderId::new("enterprise").unwrap(),
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
                prompts: Exposure::None,
                completions: CompletionExposure::Enabled,
                tasks: TaskExposure::Enabled,
            }],
            metadata: Value::Null,
        }
    }

    fn catalog() -> GatewayCatalog {
        GatewayCatalog::from_control_plane(GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            servers: vec![media_manifest()],
            profiles: vec![profile()],
            policies: vec![policy()],
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
        })
        .unwrap()
    }

    fn principal(scopes: &[&str]) -> Principal {
        Principal {
            id: PrincipalId::new("user@example.com").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            subject: TokenSubject::new("00u123").unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: scopes
                .iter()
                .map(|scope| ScopeName::new(*scope).unwrap())
                .collect(),
            data_labels: BTreeSet::<DataLabelId>::new(),
            authenticated_at: Some(
                DateTime::parse_from_rfc3339("2026-07-02T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
        }
    }

    #[test]
    fn projects_and_parses_gateway_tool_names() {
        let catalog = catalog();
        let server = ServerSlug::new("media").unwrap();
        let tool = LocalToolName::new("run").unwrap();

        let gateway_name = catalog.project_tool_name(&server, &tool).unwrap();
        let projection = catalog.parse_tool_name(&gateway_name).unwrap();

        assert_eq!(gateway_name.as_str(), "media__run");
        assert_eq!(projection.server, server);
        assert_eq!(projection.tool, tool);
    }

    #[test]
    fn policy_allows_exposed_tool_with_required_scope() {
        let catalog = catalog();
        let principal = principal(&["media:use"]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-1").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Allow);
        assert_eq!(decision.reason, PolicyReasonCode::PolicyAllow);
    }

    #[test]
    fn policy_denies_missing_required_scope() {
        let catalog = catalog();
        let principal = principal(&[]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-2").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingScope);
    }

    #[test]
    fn policy_denies_unknown_profile() {
        let catalog = catalog();
        let principal = principal(&["media:use"]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("unknown").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-3").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::UnknownProfile);
    }

    #[test]
    fn json_config_round_trips_through_contract_validation() {
        let text = serde_json::to_string(&catalog().control_plane().clone()).unwrap();
        let parsed: GatewayControlPlane = serde_json::from_str(&text).unwrap();
        let catalog = GatewayCatalog::from_control_plane(parsed).unwrap();

        assert_eq!(catalog.server_count(), 1);
        assert_eq!(catalog.profile_count(), 1);
    }

    #[test]
    fn builds_protected_resource_metadata_for_profile() {
        let catalog = catalog();
        let metadata = catalog
            .protected_resource_metadata(&GatewayProfileId::new("default").unwrap())
            .unwrap();

        assert_eq!(metadata.resource, "https://veoveo.bioma.ai/mcp/default");
        assert_eq!(
            metadata.authorization_servers,
            vec!["https://idp.example.com".to_string()]
        );
        assert_eq!(metadata.scopes_supported, vec!["media:use".to_string()]);
        assert_eq!(
            metadata.bearer_methods_supported,
            vec!["header".to_string()]
        );
    }

    #[test]
    fn builds_www_authenticate_challenge_with_scope() {
        let challenge = www_authenticate_challenge(
            "https://veoveo.bioma.ai/.well-known/oauth-protected-resource/mcp/default",
            &[ScopeName::new("media:use").unwrap()],
        );

        assert_eq!(
            challenge,
            "Bearer resource_metadata=\"https://veoveo.bioma.ai/.well-known/oauth-protected-resource/mcp/default\", scope=\"media:use\""
        );
    }

    #[test]
    fn keeps_contract_validation_errors_visible() {
        let err = GatewayCatalog::from_control_plane(GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            servers: vec![media_manifest()],
            profiles: vec![{
                let mut profile = profile();
                profile.servers[0].server = ServerSlug::new("simulation").unwrap();
                profile
            }],
            policies: vec![policy()],
            secrets: vec![],
            metadata: Value::Null,
        })
        .expect_err("unknown server should fail");

        let root = err
            .downcast_ref::<GatewayControlPlaneError>()
            .expect("contract error should be preserved");
        assert!(matches!(
            root,
            GatewayControlPlaneError::UnknownServer { .. }
        ));
    }

    #[test]
    fn task_id_type_is_available_for_runtime_state() {
        let task = GatewayTaskId::new("gateway-task-1").unwrap();
        assert_eq!(task.as_str(), "gateway-task-1");
    }
}
