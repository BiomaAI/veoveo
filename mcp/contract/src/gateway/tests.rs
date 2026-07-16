use super::*;

fn identity_provider() -> IdentityProvider {
    IdentityProvider {
        id: IdentityProviderId::new("enterprise").unwrap(),
        issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
        jwks: JwksSource::Remote {
            jwks_uri: HttpsUrl::new("https://idp.example.com/.well-known/jwks.json").unwrap(),
        },
        claim_mapping: IdentityProviderClaimMapping::default(),
        trusted_certificate_authorities: Vec::new(),
        authorization_endpoint: Some(
            OAuthEndpointUrl::new("https://idp.example.com/oauth2/authorize").unwrap(),
        ),
        token_endpoint: Some(
            OAuthEndpointUrl::new("https://idp.example.com/oauth2/token").unwrap(),
        ),
        enterprise_managed_authorization_endpoint: Some(
            OAuthEndpointUrl::new("https://idp.example.com/oauth2/id-jag").unwrap(),
        ),
        metadata: Value::Null,
    }
}

fn authorization_server() -> ResourceAuthorizationServer {
    ResourceAuthorizationServer {
        id: AuthorizationServerId::new("veoveo").unwrap(),
        issuer: TokenIssuer::new("https://veoveo.example/oauth").unwrap(),
        jwks: JwksSource::Remote {
            jwks_uri: HttpsUrl::new("https://veoveo.example/oauth/jwks.json").unwrap(),
        },
        access_token_key_id: JwtId::new("test-key").unwrap(),
        access_token_signing_key: SecretReferenceId::new("veoveo_access_token_private_key")
            .unwrap(),
        identity_provider: Some(IdentityProviderId::new("enterprise").unwrap()),
        authorization_endpoint: Some(
            OAuthEndpointUrl::new("https://veoveo.example/oauth/authorize").unwrap(),
        ),
        token_endpoint: OAuthEndpointUrl::new("https://veoveo.example/oauth/token").unwrap(),
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

fn tls_client_certificate_secret() -> SecretReference {
    SecretReference {
        id: SecretReferenceId::new("media_upstream_tls_client_certificate").unwrap(),
        source: SecretSource::Env,
        purpose: SecretPurpose::TlsClientCertificate,
        locator: SecretLocator::new("MEDIA_UPSTREAM_TLS_CLIENT_CERTIFICATE_PEM").unwrap(),
        owner: SecretOwner::Gateway,
        rotation_hint: None,
        metadata: Value::Null,
    }
}

fn tls_client_private_key_secret() -> SecretReference {
    SecretReference {
        id: SecretReferenceId::new("media_upstream_tls_client_private_key").unwrap(),
        source: SecretSource::Env,
        purpose: SecretPurpose::TlsClientPrivateKey,
        locator: SecretLocator::new("MEDIA_UPSTREAM_TLS_CLIENT_PRIVATE_KEY_PEM").unwrap(),
        owner: SecretOwner::Gateway,
        rotation_hint: None,
        metadata: Value::Null,
    }
}

fn default_secrets() -> Vec<SecretReference> {
    vec![signing_secret(), oidc_client_secret()]
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
            security: UpstreamTransportSecurity::ClusterInternalHttp,
            trusted_certificate_authorities: Vec::new(),
            client_certificate: None,
            client_private_key: None,
        },
        capabilities: McpSurfaceCapabilities {
            apps: false,
            tools: true,
            resources: true,
            resource_templates: true,
            resource_subscriptions: true,
            prompts: true,
            completions: true,
            tasks: true,
            notifications: true,
        },
        resource_projection: ResourceProjectionMode::Identity,
        tools: vec![LocalToolName::new("run").unwrap()],
        compatibility_helpers: Vec::new(),
        prompts: vec![PromptName::new("model_help").unwrap()],
        required_scopes: vec![ScopeName::new("operator:use").unwrap()],
        owned_routes: vec![OwnedRoute {
            path: MountPath::new("/media/webhooks").unwrap(),
            purpose: OwnedRoutePurpose::Webhook,
        }],
        metadata: Value::Null,
    }
}

fn control_plane_with_server_and_secrets(
    server: ServerManifest,
    secrets: Vec<SecretReference>,
) -> GatewayControlPlane {
    GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![server],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets,
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
            protected_resources: BTreeSet::new(),
            servers: BTreeSet::from([ServerSlug::new("media").unwrap()]),
            tools: BTreeSet::from([LocalToolName::new("run").unwrap()]),
            resource_schemes: BTreeSet::new(),
            prompts: BTreeSet::new(),
            principal_ids: BTreeSet::new(),
            tenant_ids: BTreeSet::new(),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            required_scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
            required_data_labels: BTreeSet::new(),
            required_assurances: BTreeSet::new(),
            metadata: Value::Null,
        }],
        metadata: Value::Null,
    }
}

fn default_data_labels() -> Vec<DataLabelDefinition> {
    vec![
        DataLabelDefinition {
            id: DataLabelId::new("cui").unwrap(),
            title: Some("Controlled Unclassified Information".to_string()),
            description: None,
            regulated: true,
            metadata: Value::Null,
        },
        DataLabelDefinition {
            id: DataLabelId::new("itar").unwrap(),
            title: Some("ITAR-controlled data".to_string()),
            description: None,
            regulated: true,
            metadata: Value::Null,
        },
        DataLabelDefinition {
            id: DataLabelId::new("pii").unwrap(),
            title: Some("Personally Identifiable Information".to_string()),
            description: None,
            regulated: true,
            metadata: Value::Null,
        },
    ]
}

fn default_tenants() -> Vec<TenantDefinition> {
    vec![TenantDefinition {
        id: TenantId::new("tenant-a").unwrap(),
        title: Some("Tenant A".to_string()),
        description: None,
        metadata: Value::Null,
    }]
}

fn default_profile() -> GatewayProfile {
    GatewayProfile {
        id: GatewayProfileId::new("default").unwrap(),
        identity_provider: IdentityProviderId::new("enterprise").unwrap(),
        authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
        protected_resource: ProtectedResourceId::new("https://veoveo.example/mcp/operator")
            .unwrap(),
        policy_version: PolicyVersion::new("2026-07-02").unwrap(),
        auth_modes: BTreeSet::from([
            AuthMode::EnterpriseManagedAuthorization,
            AuthMode::OAuthClientCredentials,
            AuthMode::OidcAuthorizationCodePkce,
        ]),
        required_scopes: vec![ScopeName::new("operator:use").unwrap()],
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

fn default_oauth_clients() -> Vec<OAuthClientRegistration> {
    vec![
        OAuthClientRegistration {
            id: OAuthClientId::new("operator-local-public").unwrap(),
            authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
            display_name: Some("Veoveo Operator Local Client".to_string()),
            client_surface: OAuthClientSurface::FullMcp,
            allowed_compatibility_helpers: BTreeSet::new(),
            direct_task_call_adapter: false,
            allowed_resources: BTreeSet::from([ProtectedResourceId::new(
                "https://veoveo.example/mcp/operator",
            )
            .unwrap()]),
            grant_types: BTreeSet::from([
                OAuthGrantType::AuthorizationCodePkce,
                OAuthGrantType::RefreshToken,
                OAuthGrantType::EnterpriseManagedAuthorization,
            ]),
            auth_methods: BTreeSet::from([OAuthClientAuthMethod::None]),
            redirect_uris: vec![
                OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap(),
                OAuthRedirectUri::new("http://127.0.0.1:8789/oauth/callback").unwrap(),
            ],
            allowed_scopes: BTreeSet::from([
                ScopeName::new("operator:use").unwrap(),
                ScopeName::new("admin:manage").unwrap(),
            ]),
            credential_secret: None,
            jwks: None,
            tenant: None,
            metadata: Value::Null,
        },
        OAuthClientRegistration {
            id: OAuthClientId::new("operator-service").unwrap(),
            authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
            display_name: Some("Veoveo Operator Service".to_string()),
            client_surface: OAuthClientSurface::FullMcp,
            allowed_compatibility_helpers: BTreeSet::new(),
            direct_task_call_adapter: false,
            allowed_resources: BTreeSet::from([ProtectedResourceId::new(
                "https://veoveo.example/mcp/operator",
            )
            .unwrap()]),
            grant_types: BTreeSet::from([OAuthGrantType::ClientCredentials]),
            auth_methods: BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt]),
            redirect_uris: vec![],
            allowed_scopes: BTreeSet::from([
                ScopeName::new("operator:use").unwrap(),
                ScopeName::new("admin:manage").unwrap(),
            ]),
            credential_secret: None,
            jwks: Some(JwksSource::Remote {
                jwks_uri: HttpsUrl::new("https://idp.example.com/oauth2/clients/jwks.json")
                    .unwrap(),
            }),
            tenant: None,
            metadata: Value::Null,
        },
    ]
}

fn default_oidc_clients() -> Vec<IdentityProviderOidcClientRegistration> {
    vec![IdentityProviderOidcClientRegistration {
        id: OidcClientRegistrationId::new("enterprise").unwrap(),
        identity_provider: IdentityProviderId::new("enterprise").unwrap(),
        authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
        allowed_resources: BTreeSet::from([ProtectedResourceId::new(
            "https://veoveo.example/mcp/operator",
        )
        .unwrap()]),
        client_id: OidcClientId::new("veoveo").unwrap(),
        redirect_uri: OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap(),
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

fn hosted_compat_oauth_client(
    helpers: BTreeSet<CompatibilityHelperId>,
    direct_task_call_adapter: bool,
) -> OAuthClientRegistration {
    OAuthClientRegistration {
        id: OAuthClientId::new("operator-hosted-public").unwrap(),
        authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
        display_name: Some("Veoveo Operator Hosted Client".to_string()),
        client_surface: OAuthClientSurface::ToolsCompat,
        allowed_compatibility_helpers: helpers,
        direct_task_call_adapter,
        allowed_resources: BTreeSet::from([ProtectedResourceId::new(
            "https://veoveo.example/mcp/operator",
        )
        .unwrap()]),
        grant_types: BTreeSet::from([OAuthGrantType::AuthorizationCodePkce]),
        auth_methods: BTreeSet::from([OAuthClientAuthMethod::None]),
        redirect_uris: vec![
            OAuthRedirectUri::new("https://claude.ai/api/mcp/auth_callback").unwrap(),
        ],
        allowed_scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
        credential_secret: None,
        jwks: None,
        tenant: None,
        metadata: Value::Null,
    }
}

#[test]
fn identifiers_reject_invalid_wire_values() {
    assert!(ServerSlug::new("Media").is_err());
    assert!(GatewayProfileId::new("default/profile").is_err());
    assert!(ResourceScheme::new("1media").is_err());
    assert!(MountPath::new("media").is_err());
    assert!(OAuthRedirectUri::new("https://veoveo.example/oauth/callback").is_ok());
    assert!(OAuthRedirectUri::new("http://127.0.0.1:8789/oauth/callback").is_ok());
    assert!(OAuthRedirectUri::new("http://[::1]:8789/oauth/callback").is_ok());
    assert!(OAuthRedirectUri::new("http://example.com/oauth/callback").is_err());
    assert!(OAuthRedirectUri::new("http://127.0.0.1/oauth/callback").is_err());
    assert!(OAuthRedirectUri::new("http://127.0.0.1:0/oauth/callback").is_err());
    assert!(OAuthStateValue::new("oauth-state-1").is_ok());
    assert!(OAuthStateValue::new("oauth state").is_err());
    assert!(OidcNonce::new("nonce-1").is_ok());
    assert!(OAuthAuthorizationCode::new("a".repeat(43)).is_ok());
    assert!(OAuthAuthorizationCode::new("short").is_err());
    assert!(PkceCodeChallenge::new("A".repeat(43)).is_ok());
    assert!(PkceCodeVerifier::new("a".repeat(129)).is_err());
    assert!(UpstreamUrl::new("http://media-mcp:8787/media/mcp").is_ok());
    assert!(UpstreamUrl::new("https://media.example.com/mcp").is_ok());
    assert!(UpstreamUrl::new("ftp://media-mcp/media/mcp").is_err());
    assert!(UpstreamUrl::new("http://user:pass@media-mcp/media/mcp").is_err());
    assert!(UpstreamUrl::new("http://media-mcp/media/mcp?debug=true").is_err());
    assert!(ResourceUri::new("media://artifact/abc").is_ok());
    assert!(ResourceUriTemplate::new("media://model").is_err());
}

#[test]
fn resource_uri_templates_match_simple_resource_uris() {
    let model_template = ResourceUriTemplate::new("media://model/{model_id}").unwrap();
    assert!(model_template.matches_uri(&ResourceUri::new("media://model/provider/model").unwrap()));
    assert!(model_template.matches_uri(&ResourceUri::new("media://model/simple").unwrap()));
    assert!(!model_template.matches_uri(&ResourceUri::new("media://models").unwrap()));
    assert!(
        !model_template
            .matches_uri(&ResourceUri::new("simulation://model/provider/model").unwrap())
    );

    let usage_template = ResourceUriTemplate::new("media://usage/task/{task_id}").unwrap();
    assert!(usage_template.matches_uri(&ResourceUri::new("media://usage/task/task-1").unwrap()));
    assert!(!usage_template.matches_uri(&ResourceUri::new("media://usage/task/").unwrap()));
    assert!(!usage_template.matches_uri(&ResourceUri::new("media://usage").unwrap()));
}

#[test]
fn resource_uri_templates_reject_unsupported_expressions() {
    assert!(ResourceUriTemplate::new("media://model/{}").is_err());
    assert!(ResourceUriTemplate::new("media://model/{+model_id}").is_err());
    assert!(ResourceUriTemplate::new("media://model/{model id}").is_err());
    assert!(ResourceUriTemplate::new("media://model/{model_id").is_err());
    assert!(ResourceUriTemplate::new("media://model/model_id}").is_err());
    assert!(ResourceUriTemplate::new("media://model/{left}{right}").is_err());
}

#[test]
fn control_plane_validates_upstream_transport_security() {
    let mut loopback_manifest = media_manifest();
    loopback_manifest.upstream.url = UpstreamUrl::new("http://127.0.0.1:18801/media/mcp").unwrap();
    loopback_manifest.upstream.security = UpstreamTransportSecurity::LoopbackHttp;
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![loopback_manifest],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    config.validate().expect("loopback HTTP upstream is valid");

    let mut mesh_manifest = media_manifest();
    mesh_manifest.upstream.url =
        UpstreamUrl::new("http://media-mcp.default.svc.cluster.local/media/mcp").unwrap();
    mesh_manifest.upstream.security = UpstreamTransportSecurity::ServiceMeshMtls;
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![mesh_manifest],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    config
        .validate()
        .expect("service-mesh internal HTTP upstream is valid");

    let mut public_plaintext_manifest = media_manifest();
    public_plaintext_manifest.upstream.url =
        UpstreamUrl::new("http://media.example.com/media/mcp").unwrap();
    public_plaintext_manifest.upstream.security = UpstreamTransportSecurity::ServiceMeshMtls;
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![public_plaintext_manifest],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("public plaintext upstream must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::ServerUpstreamSecurityMismatch { .. }
    ));
}

#[test]
fn mutual_tls_upstream_requires_typed_client_material() {
    let mut manifest = media_manifest();
    manifest.upstream.url = UpstreamUrl::new("https://media.example.com/media/mcp").unwrap();
    manifest.upstream.security = UpstreamTransportSecurity::MutualTls;

    let err = control_plane_with_server_and_secrets(manifest.clone(), default_secrets())
        .validate()
        .expect_err("mutual TLS requires client certificate and private key references");
    assert!(matches!(
        err,
        GatewayControlPlaneError::ServerUpstreamTlsSecretRequired {
            purpose: SecretPurpose::TlsClientCertificate,
            ..
        }
    ));

    manifest.upstream.client_certificate =
        Some(SecretReferenceId::new("media_upstream_tls_client_certificate").unwrap());
    let err = control_plane_with_server_and_secrets(manifest.clone(), default_secrets())
        .validate()
        .expect_err("mutual TLS also requires a client private key reference");
    assert!(matches!(
        err,
        GatewayControlPlaneError::ServerUpstreamTlsSecretRequired {
            purpose: SecretPurpose::TlsClientPrivateKey,
            ..
        }
    ));

    manifest.upstream.client_private_key =
        Some(SecretReferenceId::new("media_upstream_tls_client_private_key").unwrap());
    let err = control_plane_with_server_and_secrets(manifest.clone(), default_secrets())
        .validate()
        .expect_err("mutual TLS references must exist in the secret catalog");
    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownServerUpstreamTlsSecret { .. }
    ));

    let mut wrong_purpose = tls_client_private_key_secret();
    wrong_purpose.purpose = SecretPurpose::OAuthClientSecret;
    let err = control_plane_with_server_and_secrets(
        manifest.clone(),
        vec![
            signing_secret(),
            oidc_client_secret(),
            tls_client_certificate_secret(),
            wrong_purpose,
        ],
    )
    .validate()
    .expect_err("mutual TLS secrets must have TLS-specific purposes");
    assert!(matches!(
        err,
        GatewayControlPlaneError::ServerUpstreamTlsSecretPurposeMismatch {
            expected: SecretPurpose::TlsClientPrivateKey,
            ..
        }
    ));

    control_plane_with_server_and_secrets(
        manifest,
        vec![
            signing_secret(),
            oidc_client_secret(),
            tls_client_certificate_secret(),
            tls_client_private_key_secret(),
        ],
    )
    .validate()
    .expect("mutual TLS validates with typed certificate and private key secrets");
}

#[test]
fn non_mutual_tls_upstream_rejects_client_material() {
    let mut manifest = media_manifest();
    manifest.upstream.client_certificate =
        Some(SecretReferenceId::new("media_upstream_tls_client_certificate").unwrap());
    manifest.upstream.client_private_key =
        Some(SecretReferenceId::new("media_upstream_tls_client_private_key").unwrap());

    let err = control_plane_with_server_and_secrets(
        manifest,
        vec![
            signing_secret(),
            oidc_client_secret(),
            tls_client_certificate_secret(),
            tls_client_private_key_secret(),
        ],
    )
    .validate()
    .expect_err("client TLS material is only meaningful for mutual TLS upstreams");
    assert!(matches!(
        err,
        GatewayControlPlaneError::ServerUpstreamTlsClientMaterialNotAllowed { .. }
    ));
}

#[test]
fn auth_modes_expose_mcp_extension_ids() {
    assert_eq!(
        AuthMode::EnterpriseManagedAuthorization.mcp_extension_id(),
        Some(MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION)
    );
    assert_eq!(
        AuthMode::OAuthClientCredentials.mcp_extension_id(),
        Some(MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION)
    );
    assert_eq!(AuthMode::OidcAuthorizationCodePkce.mcp_extension_id(), None);
}

#[test]
fn gateway_actions_expose_subscription_mcp_methods() {
    assert_eq!(
        GatewayAction::ResourcesSubscribe.mcp_method(),
        Some("resources/subscribe")
    );
    assert_eq!(
        GatewayAction::ResourcesUnsubscribe.mcp_method(),
        Some("resources/unsubscribe")
    );
}

#[test]
fn control_plane_validates_cross_references() {
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
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
    };

    config.validate().expect("valid gateway control plane");
}

#[test]
fn control_plane_rejects_unknown_server_reference() {
    let mut profile = default_profile();
    profile.servers[0].server = ServerSlug::new("simulation").unwrap();
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config.validate().expect_err("unknown server must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownServer { .. }
    ));
}

#[test]
fn control_plane_rejects_duplicate_profile_server_reference() {
    let mut profile = default_profile();
    profile.servers.push(profile.servers[0].clone());
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("duplicate profile server exposure must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::DuplicateProfileServer { .. }
    ));
}

#[test]
fn control_plane_rejects_unknown_profile_tool() {
    let mut profile = default_profile();
    profile.servers[0].tools = Exposure::Listed(vec![LocalToolName::new("simulate").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown profile tool must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownProfileTool { .. }
    ));
}

#[test]
fn control_plane_rejects_unknown_profile_prompt() {
    let mut profile = default_profile();
    profile.servers[0].prompts = Exposure::Listed(vec![PromptName::new("unknown-prompt").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown profile prompt must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownProfilePrompt { .. }
    ));
}

#[test]
fn control_plane_rejects_profile_resource_scheme_mismatch() {
    let mut profile = default_profile();
    profile.servers[0].resources = Exposure::Listed(vec![ResourceSelector::Scheme {
        scheme: ResourceScheme::new("simulation").unwrap(),
    }]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("profile resource selector must stay server-scoped");

    assert!(matches!(
        err,
        GatewayControlPlaneError::ProfileResourceSelectorMismatch { .. }
    ));
}

#[test]
fn control_plane_accepts_server_owned_projected_ui_resources() {
    let mut chart_server = media_manifest();
    chart_server.slug = ServerSlug::new("charts").unwrap();
    chart_server.uri_scheme = ResourceScheme::new("charts").unwrap();
    chart_server.resource_projection = ResourceProjectionMode::ServerOwned;

    let mut profile = default_profile();
    profile.servers[0].server = ServerSlug::new("charts").unwrap();
    profile.servers[0].tools = Exposure::None;
    profile.servers[0].resources = Exposure::Listed(vec![ResourceSelector::UriPrefix {
        prefix: ResourceUriPrefix::new("ui://charts/").unwrap(),
    }]);

    let mut policy = default_policy();
    policy.rules[0].actions = BTreeSet::from([GatewayAction::ResourcesRead]);
    policy.rules[0].servers = BTreeSet::from([ServerSlug::new("charts").unwrap()]);
    policy.rules[0].tools.clear();
    policy.rules[0].resource_schemes = BTreeSet::from([ResourceScheme::new("ui").unwrap()]);

    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![chart_server],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    config
        .validate()
        .expect("server-owned projected UI resources must validate");
}

#[test]
fn control_plane_rejects_projected_ui_resources_for_other_server_slug() {
    let mut chart_server = media_manifest();
    chart_server.slug = ServerSlug::new("charts").unwrap();
    chart_server.uri_scheme = ResourceScheme::new("charts").unwrap();
    chart_server.resource_projection = ResourceProjectionMode::ServerOwned;

    let mut profile = default_profile();
    profile.servers[0].server = ServerSlug::new("charts").unwrap();
    profile.servers[0].resources = Exposure::Listed(vec![ResourceSelector::UriPrefix {
        prefix: ResourceUriPrefix::new("ui://other/").unwrap(),
    }]);

    let mut policy = default_policy();
    policy.rules[0].servers = BTreeSet::from([ServerSlug::new("charts").unwrap()]);

    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![chart_server],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("projected UI resource prefix must belong to the server slug");

    assert!(matches!(
        err,
        GatewayControlPlaneError::ProfileResourceSelectorMismatch { .. }
    ));
}

#[test]
fn control_plane_rejects_disabled_profile_capability() {
    let mut manifest = media_manifest();
    manifest.capabilities.tasks = false;
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![manifest],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("profile cannot expose disabled task capability");

    assert!(matches!(
        err,
        GatewayControlPlaneError::ProfileExposesDisabledCapability {
            capability: McpSurfaceCapability::Tasks,
            ..
        }
    ));
}

#[test]
fn control_plane_rejects_unknown_identity_provider_reference() {
    let mut profile = default_profile();
    profile.identity_provider = IdentityProviderId::new("missing").unwrap();
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown identity provider must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownIdentityProvider { .. }
    ));
}

#[test]
fn control_plane_rejects_unknown_authorization_server_reference() {
    let mut profile = default_profile();
    profile.authorization_server = AuthorizationServerId::new("missing").unwrap();
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown authorization server must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownAuthorizationServer { .. }
    ));
}

#[test]
fn control_plane_rejects_authorization_server_unknown_identity_provider() {
    let mut authorization_server = authorization_server();
    authorization_server.identity_provider = Some(IdentityProviderId::new("missing").unwrap());
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("authorization server IdP reference must be known");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownAuthorizationServerIdentityProvider { .. }
    ));
}

#[test]
fn control_plane_rejects_profile_without_auth_modes() {
    let mut profile = default_profile();
    profile.auth_modes.clear();
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config.validate().expect_err("empty auth modes must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::MissingAuthModes { .. }
    ));
}

#[test]
fn control_plane_rejects_oidc_profile_without_browser_endpoints() {
    let mut auth_server = authorization_server();
    auth_server.authorization_endpoint = None;
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::OidcAuthorizationCodePkce]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![auth_server],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("OIDC browser auth requires authorization endpoint");

    assert!(matches!(
        err,
        GatewayControlPlaneError::MissingAuthorizationServerEndpoint {
            endpoint: AuthorizationServerEndpoint::Authorization,
            ..
        }
    ));

    let mut idp = identity_provider();
    idp.token_endpoint = None;
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::OidcAuthorizationCodePkce]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![idp],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("OIDC browser auth requires token endpoint");

    assert!(matches!(
        err,
        GatewayControlPlaneError::MissingIdentityProviderEndpoint {
            endpoint: IdentityProviderEndpoint::Token,
            ..
        }
    ));
}

#[test]
fn control_plane_rejects_extension_auth_modes_without_matching_endpoints() {
    let mut idp = identity_provider();
    idp.enterprise_managed_authorization_endpoint = None;
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::EnterpriseManagedAuthorization]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![idp],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("enterprise-managed auth requires endpoint");

    assert!(matches!(
        err,
        GatewayControlPlaneError::MissingIdentityProviderEndpoint {
            endpoint: IdentityProviderEndpoint::EnterpriseManagedAuthorization,
            ..
        }
    ));
}

#[test]
fn control_plane_rejects_unknown_secret_owner_references() {
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: vec![
            signing_secret(),
            SecretReference {
                id: SecretReferenceId::new("profile_secret").unwrap(),
                source: SecretSource::Env,
                purpose: SecretPurpose::OAuthClientSecret,
                locator: SecretLocator::new("PROFILE_SECRET").unwrap(),
                owner: SecretOwner::Profile {
                    profile: GatewayProfileId::new("missing").unwrap(),
                },
                rotation_hint: None,
                metadata: Value::Null,
            },
        ],
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown profile secret owner must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownSecretOwnerProfile { .. }
    ));

    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: vec![
            signing_secret(),
            SecretReference {
                id: SecretReferenceId::new("server_secret").unwrap(),
                source: SecretSource::Env,
                purpose: SecretPurpose::ProviderApiKey,
                locator: SecretLocator::new("SERVER_SECRET").unwrap(),
                owner: SecretOwner::Server {
                    server: ServerSlug::new("missing").unwrap(),
                },
                rotation_hint: None,
                metadata: Value::Null,
            },
        ],
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown server secret owner must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownSecretOwnerServer { .. }
    ));

    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: vec![
            signing_secret(),
            SecretReference {
                id: SecretReferenceId::new("tenant_secret").unwrap(),
                source: SecretSource::Env,
                purpose: SecretPurpose::TokenExchangeCredential,
                locator: SecretLocator::new("TENANT_SECRET").unwrap(),
                owner: SecretOwner::Tenant {
                    tenant: TenantId::new("missing").unwrap(),
                },
                rotation_hint: None,
                metadata: Value::Null,
            },
        ],
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown tenant secret owner must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownSecretOwnerTenant { .. }
    ));
}

#[test]
fn control_plane_rejects_missing_oauth_client_for_auth_mode() {
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: vec![],
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("profile auth modes require OAuth clients");

    assert!(matches!(
        err,
        GatewayControlPlaneError::MissingOAuthClientForAuthMode { .. }
    ));
}

#[test]
fn control_plane_rejects_missing_oidc_client_for_browser_auth() {
    let mut profile = default_profile();
    profile.auth_modes = BTreeSet::from([AuthMode::OidcAuthorizationCodePkce]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![profile],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: vec![],
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("browser OIDC auth requires an OIDC client registration");

    assert!(matches!(
        err,
        GatewayControlPlaneError::MissingOidcClientForProfile { .. }
    ));
}

#[test]
fn control_plane_rejects_oidc_client_without_openid_scope() {
    let mut clients = default_oidc_clients();
    clients[0].scopes.remove(&ScopeName::new("openid").unwrap());
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: clients,
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("OIDC clients must request the openid scope");

    assert!(matches!(
        err,
        GatewayControlPlaneError::OidcClientMissingOpenIdScope(_)
    ));
}

#[test]
fn control_plane_rejects_public_client_credentials() {
    let mut clients = default_oauth_clients();
    clients[1].auth_methods = BTreeSet::from([OAuthClientAuthMethod::None]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("client credentials must not be public");

    assert!(matches!(
        err,
        GatewayControlPlaneError::OAuthClientPublicClientCredentials(_)
    ));
}

#[test]
fn control_plane_rejects_private_key_jwt_without_jwks() {
    let mut clients = default_oauth_clients();
    clients[1].jwks = None;
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("private-key JWT clients require a JWKS source");

    assert!(matches!(
        err,
        GatewayControlPlaneError::OAuthClientMissingJwks { .. }
    ));
}

#[test]
fn control_plane_rejects_unsupported_oauth_client_auth_combinations() {
    let mut clients = default_oauth_clients();
    clients[0].auth_methods = BTreeSet::from([
        OAuthClientAuthMethod::None,
        OAuthClientAuthMethod::PrivateKeyJwt,
    ]);
    clients[0].jwks = Some(JwksSource::Remote {
        jwks_uri: HttpsUrl::new("https://idp.example.com/oauth2/browser/jwks.json").unwrap(),
    });
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("browser OAuth client must remain public none-auth");

    assert!(matches!(
        err,
        GatewayControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. }
    ));

    let mut clients = default_oauth_clients();
    clients[0].grant_types = BTreeSet::from([OAuthGrantType::RefreshToken]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("refresh tokens require an authorization-code grant on the same client");
    assert!(matches!(
        err,
        GatewayControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. }
    ));

    let mut clients = default_oauth_clients();
    clients[1].auth_methods = BTreeSet::from([OAuthClientAuthMethod::ClientSecretPost]);
    clients[1].credential_secret =
        Some(SecretReferenceId::new("enterprise_oidc_client_secret").unwrap());
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("client credentials must use private-key JWT");

    assert!(matches!(
        err,
        GatewayControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. }
    ));

    let mut clients = default_oauth_clients();
    clients[1].grant_types = BTreeSet::from([
        OAuthGrantType::AuthorizationCodePkce,
        OAuthGrantType::ClientCredentials,
    ]);
    clients[1].redirect_uris =
        vec![OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap()];
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("one OAuth client must not mix browser and client credentials grants");

    assert!(matches!(
        err,
        GatewayControlPlaneError::OAuthClientUnsupportedAuthConfiguration { .. }
    ));
}

#[test]
fn control_plane_rejects_oauth_client_missing_required_scope() {
    let mut clients = default_oauth_clients();
    clients[0]
        .allowed_scopes
        .remove(&ScopeName::new("operator:use").unwrap());
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: clients,
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("client allowed scopes must cover profile and policy scopes");

    assert!(matches!(
        err,
        GatewayControlPlaneError::OAuthClientMissingAllowedScope { .. }
    ));
}

#[test]
fn control_plane_rejects_duplicate_resource_schemes() {
    let mut second_server = media_manifest();
    second_server.slug = ServerSlug::new("simulation").unwrap();
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest(), second_server],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("duplicate resource schemes must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::DuplicateResourceScheme(_)
    ));
}

#[test]
fn control_plane_rejects_duplicate_tenants() {
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: vec![
            TenantDefinition {
                id: TenantId::new("tenant-a").unwrap(),
                title: Some("Tenant A".to_string()),
                description: None,
                metadata: Value::Null,
            },
            TenantDefinition {
                id: TenantId::new("tenant-a").unwrap(),
                title: Some("Tenant A duplicate".to_string()),
                description: None,
                metadata: Value::Null,
            },
        ],
        policies: vec![default_policy()],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config.validate().expect_err("duplicate tenants must fail");

    assert!(matches!(err, GatewayControlPlaneError::DuplicateTenant(_)));
}

#[test]
fn control_plane_rejects_duplicate_policy_rule_ids() {
    let mut policy = default_policy();
    policy.rules.push(policy.rules[0].clone());
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("duplicate policy rule ids must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::DuplicatePolicyRule { .. }
    ));
}

#[test]
fn control_plane_rejects_unknown_policy_rule_references() {
    let mut policy = default_policy();
    policy.rules[0].profiles = BTreeSet::from([GatewayProfileId::new("missing").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown policy profile must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownPolicyRuleProfile { .. }
    ));

    let mut policy = default_policy();
    policy.rules[0].servers = BTreeSet::from([ServerSlug::new("missing").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown policy server must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownPolicyRuleServer { .. }
    ));

    let mut policy = default_policy();
    policy.rules[0].resource_schemes = BTreeSet::from([ResourceScheme::new("simulation").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown policy resource scheme must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownPolicyRuleResourceScheme { .. }
    ));

    let mut policy = default_policy();
    policy.rules[0].tools = BTreeSet::from([LocalToolName::new("simulate").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown policy tool must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownPolicyRuleTool { .. }
    ));

    let mut policy = default_policy();
    policy.rules[0].prompts = BTreeSet::from([PromptName::new("unknown-prompt").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown policy prompt must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownPolicyRulePrompt { .. }
    ));

    let mut policy = default_policy();
    policy.rules[0].required_data_labels =
        BTreeSet::from([DataLabelId::new("unknown_label").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown policy data label must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownPolicyRuleDataLabel { .. }
    ));

    let mut policy = default_policy();
    policy.rules[0].tenant_ids = BTreeSet::from([TenantId::new("missing").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest()],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("unknown policy tenant must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::UnknownPolicyRuleTenant { .. }
    ));

    let mut simulation_server = media_manifest();
    simulation_server.slug = ServerSlug::new("simulation").unwrap();
    simulation_server.uri_scheme = ResourceScheme::new("simulation").unwrap();
    let mut policy = default_policy();
    policy.rules[0].resource_schemes = BTreeSet::from([ResourceScheme::new("simulation").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![media_manifest(), simulation_server],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("policy resource scheme outside server scope must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::PolicyRuleResourceSchemeOutsideServerScope { .. }
    ));
}

#[test]
fn control_plane_rejects_policy_action_outside_server_capabilities() {
    let mut manifest = media_manifest();
    manifest.capabilities.resource_subscriptions = false;
    let mut policy = default_policy();
    policy.rules[0].actions = BTreeSet::from([GatewayAction::ResourcesUnsubscribe]);
    policy.rules[0].tools.clear();
    policy.rules[0].resource_schemes = BTreeSet::from([ResourceScheme::new("media").unwrap()]);
    let config = GatewayControlPlane {
        branding: None,
        identity_providers: vec![identity_provider()],
        authorization_servers: vec![authorization_server()],
        servers: vec![manifest],
        profiles: vec![default_profile()],
        recording_ingest_resources: Vec::new(),
        tenants: default_tenants(),
        policies: vec![policy],
        data_labels: default_data_labels(),
        oauth_clients: default_oauth_clients(),
        oidc_clients: default_oidc_clients(),
        secrets: default_secrets(),
        metadata: Value::Null,
    };

    let err = config
        .validate()
        .expect_err("policy action outside server capabilities must fail");

    assert!(matches!(
        err,
        GatewayControlPlaneError::PolicyRuleActionUnsupportedByServerScope {
            action: GatewayAction::ResourcesUnsubscribe,
            ..
        }
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

#[test]
fn tools_compat_client_accepts_task_projection_without_a_helper_tool() {
    let mut config = control_plane_with_server_and_secrets(media_manifest(), default_secrets());
    config
        .oauth_clients
        .push(hosted_compat_oauth_client(BTreeSet::new(), true));

    config
        .validate()
        .expect("internal task projection should not require a public helper tool");
}

#[test]
fn refresh_tokens_are_redacted_from_diagnostics_but_serialize_on_the_wire() {
    let raw = "R".repeat(43);
    let token = OAuthRefreshToken::new(raw.clone()).unwrap();

    assert_eq!(token.to_string(), "[REDACTED]");
    assert!(!format!("{token:?}").contains(&raw));
    assert_eq!(serde_json::to_value(token).unwrap(), serde_json::json!(raw));
}
