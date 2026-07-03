use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, Utc};
use rmcp::handler::server::ServerHandler;
use serde_json::Value;
use veoveo_mcp_contract::{
    AuthMode, AuthorizationServerId, CompletionExposure, DataLabelId, Exposure,
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayAction, GatewayControlPlaneError,
    GatewayInternalTokenIssuer, GatewayTaskId, GroupId, HttpsUrl, IdentityProvider,
    IdentityProviderId, IdentityProviderOidcClientRegistration, InternalTokenSecret, JwksSource,
    JwtId, LocalToolName, MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION,
    MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION, MountPath, OAuthClientAuthMethod, OAuthClientId,
    OAuthClientRegistration, OAuthGrantType, OAuthRedirectUri, OidcClientAuthMethod, OidcClientId,
    OidcClientRegistrationId, OwnedRoute, OwnedRoutePurpose, PolicyEffect, PolicyReasonCode,
    PolicyRule, PolicyRuleId, PolicyTarget, Principal, PrincipalId, PrincipalKind,
    ProfileServerExposure, ProtectedResourceId, ResourceAuthorizationServer, ResourceScheme,
    ResourceSelector, ResourceUri, ResourceUriTemplate, RoleId, ScopeName, SecretLocator,
    SecretOwner, SecretPurpose, SecretReference, SecretReferenceId, SecretSource, TaskExposure,
    TenantId, TokenIssuer, TokenSubject, TraceId, UpstreamEndpoint, UpstreamTransport,
    UpstreamTransportSecurity, UpstreamUrl,
};

use super::*;
use crate::{GatewayMcp, GatewayState, PolicyRequest, www_authenticate_challenge};

fn identity_provider() -> IdentityProvider {
    IdentityProvider {
        id: IdentityProviderId::new("enterprise").unwrap(),
        issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
        jwks: JwksSource::Remote {
            jwks_uri: HttpsUrl::new("https://idp.example.com/.well-known/jwks.json").unwrap(),
        },
        trusted_certificate_authorities: Vec::new(),
        authorization_endpoint: Some(
            HttpsUrl::new("https://idp.example.com/oauth2/authorize").unwrap(),
        ),
        token_endpoint: Some(HttpsUrl::new("https://idp.example.com/oauth2/token").unwrap()),
        enterprise_managed_authorization_endpoint: Some(
            HttpsUrl::new("https://idp.example.com/oauth2/id-jag").unwrap(),
        ),
        metadata: Value::Null,
    }
}

fn authorization_server() -> ResourceAuthorizationServer {
    ResourceAuthorizationServer {
        id: AuthorizationServerId::new("veoveo").unwrap(),
        issuer: TokenIssuer::new("https://veoveo.bioma.ai/oauth/default").unwrap(),
        jwks: JwksSource::Remote {
            jwks_uri: HttpsUrl::new("https://veoveo.bioma.ai/oauth/default/jwks.json").unwrap(),
        },
        access_token_key_id: JwtId::new("test-key").unwrap(),
        access_token_signing_key: SecretReferenceId::new("veoveo_access_token_private_key")
            .unwrap(),
        identity_provider: Some(IdentityProviderId::new("enterprise").unwrap()),
        authorization_endpoint: Some(
            HttpsUrl::new("https://veoveo.bioma.ai/oauth/default/authorize").unwrap(),
        ),
        token_endpoint: HttpsUrl::new("https://veoveo.bioma.ai/oauth/default/token").unwrap(),
        metadata: Value::Null,
    }
}

fn signing_secret() -> SecretReference {
    SecretReference {
        id: SecretReferenceId::new("veoveo_access_token_private_key").unwrap(),
        source: SecretSource::Env,
        purpose: SecretPurpose::JwksPrivateKey,
        locator: SecretLocator::new("VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64").unwrap(),
        owner: SecretOwner::Gateway,
        rotation_hint: None,
        metadata: Value::Null,
    }
}

fn oidc_client_secret() -> SecretReference {
    SecretReference {
        id: SecretReferenceId::new("enterprise_oidc_client_secret").unwrap(),
        source: SecretSource::Env,
        purpose: SecretPurpose::OAuthClientSecret,
        locator: SecretLocator::new("VEOVEO_IDP_OIDC_CLIENT_SECRET").unwrap(),
        owner: SecretOwner::Gateway,
        rotation_hint: None,
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
            url: UpstreamUrl::new("http://media-mcp:8787/media/mcp").unwrap(),
            security: UpstreamTransportSecurity::ComposeInternalHttp,
            trusted_certificate_authorities: Vec::new(),
            client_certificate: None,
            client_private_key: None,
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
        authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
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

fn oauth_clients() -> Vec<OAuthClientRegistration> {
    vec![
        OAuthClientRegistration {
            id: OAuthClientId::new("veoveo-browser").unwrap(),
            authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
            display_name: Some("Veoveo browser client".to_string()),
            allowed_profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
            grant_types: BTreeSet::from([
                OAuthGrantType::AuthorizationCodePkce,
                OAuthGrantType::EnterpriseManagedAuthorization,
            ]),
            auth_methods: BTreeSet::from([OAuthClientAuthMethod::None]),
            redirect_uris: vec![
                OAuthRedirectUri::new("https://veoveo.bioma.ai/oauth/callback").unwrap(),
                OAuthRedirectUri::new("http://127.0.0.1:8789/oauth/callback").unwrap(),
            ],
            allowed_scopes: BTreeSet::from([
                ScopeName::new("media:use").unwrap(),
                ScopeName::new("media:admin").unwrap(),
                ScopeName::new("gateway:admin").unwrap(),
            ]),
            credential_secret: None,
            jwks: None,
            metadata: Value::Null,
        },
        OAuthClientRegistration {
            id: OAuthClientId::new("veoveo-headless").unwrap(),
            authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
            display_name: Some("Veoveo headless client".to_string()),
            allowed_profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
            grant_types: BTreeSet::from([OAuthGrantType::ClientCredentials]),
            auth_methods: BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt]),
            redirect_uris: vec![],
            allowed_scopes: BTreeSet::from([
                ScopeName::new("media:use").unwrap(),
                ScopeName::new("media:admin").unwrap(),
                ScopeName::new("gateway:admin").unwrap(),
            ]),
            credential_secret: None,
            jwks: Some(JwksSource::Remote {
                jwks_uri: HttpsUrl::new("https://idp.example.com/oauth2/clients/jwks.json")
                    .unwrap(),
            }),
            metadata: Value::Null,
        },
    ]
}

fn oidc_clients() -> Vec<IdentityProviderOidcClientRegistration> {
    vec![IdentityProviderOidcClientRegistration {
        id: OidcClientRegistrationId::new("enterprise").unwrap(),
        identity_provider: IdentityProviderId::new("enterprise").unwrap(),
        authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
        client_id: OidcClientId::new("veoveo-gateway").unwrap(),
        redirect_uri: OAuthRedirectUri::new("https://veoveo.bioma.ai/oauth/default/callback")
            .unwrap(),
        auth_method: OidcClientAuthMethod::ClientSecretPost,
        credential_secret: SecretReferenceId::new("enterprise_oidc_client_secret").unwrap(),
        scopes: BTreeSet::from([
            ScopeName::new("openid").unwrap(),
            ScopeName::new("profile").unwrap(),
            ScopeName::new("email").unwrap(),
        ]),
        metadata: Value::Null,
    }]
}

fn catalog() -> GatewayCatalog {
    catalog_with_policy(policy())
}

fn catalog_with_policy(policy: PolicySet) -> GatewayCatalog {
    catalog_with_profile_and_policy(profile(), policy)
}

fn catalog_with_profile_and_policy(profile: GatewayProfile, policy: PolicySet) -> GatewayCatalog {
    GatewayCatalog::from_control_plane(GatewayControlPlane {
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        policies: vec![policy],
        oauth_clients: oauth_clients(),
        oidc_clients: oidc_clients(),
        secrets: vec![
            signing_secret(),
            oidc_client_secret(),
            SecretReference {
                id: SecretReferenceId::new("media_provider_key").unwrap(),
                source: SecretSource::Env,
                purpose: SecretPurpose::ProviderApiKey,
                locator: SecretLocator::new("MEDIA_PROVIDER_API_KEY").unwrap(),
                owner: SecretOwner::Server {
                    server: ServerSlug::new("media").unwrap(),
                },
                rotation_hint: None,
                metadata: Value::Null,
            },
        ],
        metadata: Value::Null,
    })
    .unwrap()
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let unique = uuid::Uuid::new_v4();
    std::env::temp_dir().join(format!("veoveo-gateway-lib-{name}-{unique}.duckdb"))
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
fn policy_allows_template_exposed_resource_uri() {
    let mut profile = profile();
    profile.servers[0].resources = Exposure::Listed(vec![ResourceSelector::Template {
        uri_template: ResourceUriTemplate::new("media://usage/task/{task_id}").unwrap(),
    }]);
    let mut policy = policy();
    policy.rules[0].actions = BTreeSet::from([GatewayAction::UsageRead]);
    policy.rules[0].tools.clear();
    policy.rules[0].resource_schemes = BTreeSet::from([ResourceScheme::new("media").unwrap()]);
    let catalog = catalog_with_profile_and_policy(profile, policy);
    let principal = principal(&["media:use"]);

    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::UsageRead,
        target: &PolicyTarget::Usage {
            server: ServerSlug::new("media").unwrap(),
            usage_uri: ResourceUri::new("media://usage/task/task-1").unwrap(),
        },
        trace_id: &TraceId::new("trace-template-allow").unwrap(),
    });
    assert_eq!(decision.effect, PolicyEffect::Allow);
    assert_eq!(decision.reason, PolicyReasonCode::PolicyAllow);

    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::UsageRead,
        target: &PolicyTarget::Usage {
            server: ServerSlug::new("media").unwrap(),
            usage_uri: ResourceUri::new("media://usage").unwrap(),
        },
        trace_id: &TraceId::new("trace-template-deny").unwrap(),
    });
    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::PolicyDeny);
}

#[test]
fn policy_allows_task_list_without_fake_task_id() {
    let mut policy = policy();
    policy.rules[0].actions = BTreeSet::from([GatewayAction::TasksList]);
    policy.rules[0].tools = BTreeSet::new();
    let catalog = catalog_with_policy(policy);
    let principal = principal(&["media:use"]);
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::TasksList,
        target: &PolicyTarget::TaskList {
            server: ServerSlug::new("media").unwrap(),
        },
        trace_id: &TraceId::new("trace-task-list").unwrap(),
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
fn policy_denies_missing_rule_required_scope_with_specific_reason() {
    let mut policy = policy();
    policy.rules[0].required_scopes = BTreeSet::from([ScopeName::new("media:admin").unwrap()]);
    let catalog = catalog_with_policy(policy);
    let principal = principal(&["media:use"]);
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: LocalToolName::new("run").unwrap(),
        },
        trace_id: &TraceId::new("trace-rule-scope").unwrap(),
    });

    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::MissingScope);
    assert_eq!(
        decision.rule_id,
        Some(PolicyRuleId::new("allow_media_run").unwrap())
    );
}

#[test]
fn policy_denies_missing_required_data_label_with_specific_reason() {
    let mut policy = policy();
    policy.rules[0].required_data_labels = BTreeSet::from([DataLabelId::new("cui").unwrap()]);
    let catalog = catalog_with_policy(policy);
    let mut principal = principal(&["media:use"]);
    let target = PolicyTarget::Tool {
        server: ServerSlug::new("media").unwrap(),
        tool: LocalToolName::new("run").unwrap(),
    };
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &target,
        trace_id: &TraceId::new("trace-label-deny").unwrap(),
    });

    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::MissingDataLabel);
    assert_eq!(
        decision.rule_id,
        Some(PolicyRuleId::new("allow_media_run").unwrap())
    );

    principal.data_labels = BTreeSet::from([DataLabelId::new("cui").unwrap()]);
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &target,
        trace_id: &TraceId::new("trace-label-allow").unwrap(),
    });

    assert_eq!(decision.effect, PolicyEffect::Allow);
    assert_eq!(decision.reason, PolicyReasonCode::PolicyAllow);
}

#[test]
fn policy_denies_missing_tenant_with_specific_reason() {
    let catalog = catalog();
    let mut principal = principal(&["media:use"]);
    principal.tenant = Some(TenantId::new("tenant-b").unwrap());
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: LocalToolName::new("run").unwrap(),
        },
        trace_id: &TraceId::new("trace-tenant-deny").unwrap(),
    });

    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::MissingTenant);
    assert_eq!(
        decision.rule_id,
        Some(PolicyRuleId::new("allow_media_run").unwrap())
    );
}

#[test]
fn policy_denies_missing_group_and_role_with_specific_reasons() {
    let target = PolicyTarget::Tool {
        server: ServerSlug::new("media").unwrap(),
        tool: LocalToolName::new("run").unwrap(),
    };

    let mut group_policy = policy();
    group_policy.rules[0].groups = BTreeSet::from([GroupId::new("engineering").unwrap()]);
    let group_catalog = catalog_with_policy(group_policy);
    let mut principal = principal(&["media:use"]);
    let decision = group_catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &target,
        trace_id: &TraceId::new("trace-group-deny").unwrap(),
    });
    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::MissingGroup);

    principal.groups = BTreeSet::from([GroupId::new("engineering").unwrap()]);
    let decision = group_catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &target,
        trace_id: &TraceId::new("trace-group-allow").unwrap(),
    });
    assert_eq!(decision.effect, PolicyEffect::Allow);

    let mut role_policy = policy();
    role_policy.rules[0].roles = BTreeSet::from([RoleId::new("operator").unwrap()]);
    let role_catalog = catalog_with_policy(role_policy);
    let decision = role_catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &target,
        trace_id: &TraceId::new("trace-role-deny").unwrap(),
    });
    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::MissingRole);

    principal.roles = BTreeSet::from([RoleId::new("operator").unwrap()]);
    let decision = role_catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &target,
        trace_id: &TraceId::new("trace-role-allow").unwrap(),
    });
    assert_eq!(decision.effect, PolicyEffect::Allow);
}

#[test]
fn policy_denies_missing_principal_allowlist_with_specific_reason() {
    let mut policy = policy();
    policy.rules[0].principal_ids =
        BTreeSet::from([PrincipalId::new("allowed@example.com").unwrap()]);
    let catalog = catalog_with_policy(policy);
    let principal = principal(&["media:use"]);
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: LocalToolName::new("run").unwrap(),
        },
        trace_id: &TraceId::new("trace-principal-deny").unwrap(),
    });

    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::MissingPrincipal);
    assert_eq!(
        decision.rule_id,
        Some(PolicyRuleId::new("allow_media_run").unwrap())
    );
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
fn catalog_handle_reads_replaced_catalog_with_new_generation() {
    let handle = GatewayCatalogHandle::new(Arc::new(catalog()));
    let principal = principal(&["media:use"]);
    let target = PolicyTarget::Tool {
        server: ServerSlug::new("media").unwrap(),
        tool: LocalToolName::new("run").unwrap(),
    };
    let first = handle.snapshot();
    let first_decision = first.catalog().decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &target,
        trace_id: &TraceId::new("trace-live-before").unwrap(),
    });

    assert_eq!(first.generation(), 0);
    assert_eq!(first_decision.effect, PolicyEffect::Allow);

    let mut denied_policy = policy();
    denied_policy.rules.clear();
    handle.replace(Arc::new(catalog_with_policy(denied_policy)));
    let second = handle.snapshot();
    let second_decision = second.catalog().decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::ToolsCall,
        target: &target,
        trace_id: &TraceId::new("trace-live-after").unwrap(),
    });

    assert_eq!(second.generation(), 1);
    assert_eq!(second_decision.effect, PolicyEffect::Deny);
}

#[test]
fn gateway_mcp_reads_replaced_catalog_from_existing_handler() {
    let initial_catalog = Arc::new(catalog());
    let mut replacement_control_plane = initial_catalog.control_plane().clone();
    replacement_control_plane.profiles[0].servers.clear();
    replacement_control_plane.policies[0].rules.clear();
    let replacement_catalog =
        Arc::new(GatewayCatalog::from_control_plane(replacement_control_plane).unwrap());
    let handle = GatewayCatalogHandle::new(initial_catalog);
    let state = GatewayState::open(temp_path("live-catalog")).unwrap();
    let internal_token_issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
        InternalTokenSecret::new("local-dev-internal-token-secret-32-bytes-minimum").unwrap(),
    );
    let mcp = GatewayMcp::new(
        handle.clone(),
        GatewayProfileId::new("default").unwrap(),
        state,
        internal_token_issuer,
    );

    assert!(mcp.get_info().capabilities.tools.is_some());

    handle.replace(replacement_catalog);

    assert!(mcp.get_info().capabilities.tools.is_none());
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
        vec!["https://veoveo.bioma.ai/oauth/default".to_string()]
    );
    assert_eq!(metadata.scopes_supported, vec!["media:use".to_string()]);
    assert_eq!(
        metadata.bearer_methods_supported,
        vec!["header".to_string()]
    );
    assert!(
        metadata
            .extensions
            .contains_key(MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION)
    );
    assert!(
        metadata
            .extensions
            .contains_key(MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION)
    );
}

#[test]
fn builds_authorization_server_metadata_for_profile() {
    let catalog = catalog();
    let metadata = catalog
        .authorization_server_metadata(&GatewayProfileId::new("default").unwrap())
        .unwrap();

    assert_eq!(metadata.issuer, "https://veoveo.bioma.ai/oauth/default");
    assert_eq!(
        metadata.authorization_endpoint.as_deref(),
        Some("https://veoveo.bioma.ai/oauth/default/authorize")
    );
    assert_eq!(
        metadata.token_endpoint,
        "https://veoveo.bioma.ai/oauth/default/token"
    );
    assert_eq!(
        metadata.jwks_uri.as_deref(),
        Some("https://veoveo.bioma.ai/oauth/default/jwks.json")
    );
    assert_eq!(metadata.scopes_supported, vec!["media:use".to_string()]);
    assert_eq!(metadata.response_types_supported, vec!["code".to_string()]);
    assert!(
        metadata
            .grant_types_supported
            .contains(&"authorization_code".to_string())
    );
    assert!(
        metadata
            .grant_types_supported
            .contains(&"client_credentials".to_string())
    );
    assert!(
        metadata
            .grant_types_supported
            .contains(&"urn:ietf:params:oauth:grant-type:jwt-bearer".to_string())
    );
    assert_eq!(
        metadata.code_challenge_methods_supported,
        vec!["S256".to_string()]
    );
    assert!(
        metadata
            .token_endpoint_auth_methods_supported
            .contains(&"private_key_jwt".to_string())
    );
    assert!(
        metadata
            .token_endpoint_auth_methods_supported
            .contains(&"none".to_string())
    );
    assert!(
        metadata
            .token_endpoint_auth_signing_alg_values_supported
            .contains(&"RS256".to_string())
    );
    assert_eq!(
        metadata.authorization_grant_profiles_supported,
        vec!["urn:ietf:params:oauth:grant-profile:id-jag".to_string()]
    );
}

#[test]
fn protected_resource_metadata_includes_policy_required_scopes() {
    let mut policy = policy();
    policy.rules.push(PolicyRule {
        id: PolicyRuleId::new("allow_gateway_admin_write").unwrap(),
        effect: PolicyEffect::Allow,
        actions: BTreeSet::from([GatewayAction::AdminWrite]),
        profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
        servers: BTreeSet::new(),
        tools: BTreeSet::new(),
        resource_schemes: BTreeSet::new(),
        prompts: BTreeSet::new(),
        principal_ids: BTreeSet::new(),
        tenant_ids: BTreeSet::new(),
        groups: BTreeSet::new(),
        roles: BTreeSet::new(),
        required_scopes: BTreeSet::from([ScopeName::new("gateway:admin").unwrap()]),
        required_data_labels: BTreeSet::new(),
        metadata: Value::Null,
    });
    let catalog = catalog_with_policy(policy);
    let metadata = catalog
        .protected_resource_metadata(&GatewayProfileId::new("default").unwrap())
        .unwrap();

    assert!(
        metadata
            .scopes_supported
            .iter()
            .any(|scope| scope == "media:use")
    );
    assert!(
        metadata
            .scopes_supported
            .iter()
            .any(|scope| scope == "gateway:admin")
    );
}

#[test]
fn gateway_policy_target_ignores_filtered_admin_rules() {
    let mut policy = policy();
    policy.rules[0].actions = BTreeSet::from([GatewayAction::AdminWrite]);
    let catalog = catalog_with_policy(policy);
    let principal = principal(&["media:use"]);
    let decision = catalog.decide(PolicyRequest {
        principal: &principal,
        profile: &GatewayProfileId::new("default").unwrap(),
        action: GatewayAction::AdminWrite,
        target: &PolicyTarget::Gateway,
        trace_id: &TraceId::new("trace-admin-filtered").unwrap(),
    });

    assert_eq!(decision.effect, PolicyEffect::Deny);
    assert_eq!(decision.reason, PolicyReasonCode::PolicyDeny);
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
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![{
            let mut profile = profile();
            profile.servers[0].server = ServerSlug::new("simulation").unwrap();
            profile
        }],
        policies: vec![policy()],
        oauth_clients: oauth_clients(),
        oidc_clients: oidc_clients(),
        secrets: vec![signing_secret(), oidc_client_secret()],
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
