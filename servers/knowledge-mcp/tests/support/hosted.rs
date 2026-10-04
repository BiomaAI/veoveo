use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{TimeDelta, Utc};
use rmcp::{
    ClientServiceExt,
    model::*,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use veoveo_knowledge_mcp::{contract::*, embed::Embeddings, mcp::KnowledgeMcp};
use veoveo_mcp_contract::*;
use veoveo_platform_store::{
    GatewayControlRevisionContent, GatewayControlRevisionSource, OpenObject, PlatformStore,
    RecordId,
};
use veoveo_types::{
    InvocationMode, InvocationProvenance, ScopeDefinition, Sha256Digest, WorkContextMembershipLevel,
};

pub fn plane(registrations: &[CollectionRegistration]) -> GatewayControlPlane {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../../../../configs/gateway.smoke.json")).unwrap();
    let mut server = value["servers"][0].clone();
    server["slug"] = "knowledge".into();
    server["uri_scheme"] = "knowledge".into();
    server["mount_path"] = "/knowledge".into();
    server["mcp_path"] = "/knowledge/mcp".into();
    server["upstream"]["url"] = "http://127.0.0.1:18802/knowledge/mcp".into();
    server["upstream"]["health_url"] = "http://127.0.0.1:18802/knowledge/readyz".into();
    server["owned_routes"] = serde_json::json!([]);
    server["tools"] = serde_json::json!(["search", "embed"]);
    server["prompts"] = serde_json::json!([]);
    server["compatibility_helpers"] = serde_json::json!([]);
    server["capabilities"] = serde_json::json!({"tools":true,"resources":true,"resource_templates":true,"resource_subscriptions":true,"prompts":false,"completions":true,"tasks":false});
    value["servers"].as_array_mut().unwrap().push(server);
    value["servers"][0]["knowledge"] = serde_json::to_value(
        registrations
            .iter()
            .map(|r| &r.approval)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    value["profiles"][0]["servers"].as_array_mut().unwrap().push(serde_json::json!({"server":"knowledge", "tools":{"mode":"all"}, "resources":{"mode":"all"}, "prompts":{"mode":"none"}, "completions":"enabled", "tasks":"disabled"}));
    let mut rule = value["policies"][0]["rules"][0].clone();
    rule["id"] = "allow_knowledge".into();
    rule["servers"] = serde_json::json!(["knowledge"]);
    rule["resource_schemes"] = serde_json::json!(["knowledge"]);
    rule["tools"] = serde_json::json!(["search", "embed"]);
    rule["prompts"] = serde_json::json!([]);
    rule["actions"] = serde_json::json!([
        "tools_list",
        "tools_call",
        "resources_list",
        "resources_templates_list",
        "resources_read",
        "subscriptions_listen",
        "completion_complete"
    ]);
    value["policies"][0]["rules"]
        .as_array_mut()
        .unwrap()
        .push(rule.clone());
    // Catalog observation is a server target. A resource-scoped read rule does
    // not grant the separate catalog subscription permission.
    rule["id"] = "allow_knowledge_catalog_observation".into();
    rule["actions"] = serde_json::json!(["subscriptions_listen"]);
    rule["tools"] = serde_json::json!([]);
    rule["resource_schemes"] = serde_json::json!([]);
    value["policies"][0]["rules"]
        .as_array_mut()
        .unwrap()
        .push(rule);
    for client in value["oauth_clients"].as_array_mut().unwrap() {
        if client["id"] == "operator-service" || client["id"] == "operator-local-public" {
            client["allowed_scopes"].as_array_mut().unwrap().extend(
                KnowledgeScope::ALL
                    .iter()
                    .map(|s| serde_json::Value::String(s.name().to_string())),
            );
        }
    }
    let plane: GatewayControlPlane = serde_json::from_value(value).unwrap();
    veoveo_policy::PolicyCatalog::new(plane.clone(), veoveo_gateway_catalog::registry().unwrap())
        .unwrap();
    plane
}

pub async fn install(store: &PlatformStore, plane: &GatewayControlPlane) {
    // Match GatewayCatalog publication: opaque maps must survive Store ordering
    // even when aggregate dependencies enable serde_json's preserve_order.
    let mut document = serde_json::to_value(plane).unwrap();
    document.sort_all_objects();
    let plane: GatewayControlPlane = serde_json::from_value(document).unwrap();
    let name = uuid::Uuid::now_v7().to_string();
    let revision = RecordId::new("gateway_control_revision", name.clone());
    let content = GatewayControlRevisionContent {
        revision_id: name.clone(),
        sha256: Sha256Digest::from_bytes(
            Sha256::digest(serde_json::to_vec(&plane).unwrap()).into(),
        )
        .hex()
        .to_owned(),
        source: GatewayControlRevisionSource::SeedFile,
        applied_at: Utc::now(),
        applied_by: "knowledge-fixture".into(),
        tenant: None,
        control_plane: serde_json::from_value(serde_json::to_value(plane).unwrap()).unwrap(),
    };
    store.client().query("BEGIN TRANSACTION; CREATE ONLY $revision CONTENT $content; UPSERT gateway_control_active:current SET revision=$revision, revision_id=$name, updated_at=time::now(); COMMIT TRANSACTION;")
        .bind(("revision",revision)).bind(("content", content)).bind(("name",name)).await.unwrap().check().unwrap();
    let stored = store
        .active_gateway_control_revision()
        .await
        .unwrap_or_else(|_| panic!("fixture active control revision read failed"))
        .expect("fixture active control revision");
    let restored: GatewayControlPlane =
        serde_json::from_value(serde_json::to_value(stored.control_plane).unwrap())
            .unwrap_or_else(|_| panic!("fixture stored control-plane admission failed"));
    veoveo_policy::PolicyCatalog::new(
        restored.clone(),
        veoveo_gateway_catalog::registry().unwrap(),
    )
    .unwrap_or_else(|_| panic!("fixture stored policy admission failed"));
    let digest =
        Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&restored).unwrap()).into());
    assert!(
        digest.hex() == stored.sha256,
        "fixture stored control-plane digest must match publication"
    );
}

pub struct Signing {
    issuer: GatewayInternalTokenIssuer,
    pub trust: GatewayInternalTrustBundle,
}
impl Signing {
    pub fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let trust = GatewayInternalTrustBundle::from_json(&serde_json::json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key_raw()),"alg":"EdDSA","use":"sig","kid":"fixture"}]}).to_string()).unwrap();
        let issuer = TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap();
        Self {
            issuer: GatewayInternalTokenIssuer::new(
                issuer,
                GatewayInternalSigningKey::new("fixture", key.serialize_der()).unwrap(),
            ),
            trust,
        }
    }
    pub fn issue(&self, identity: GatewayInternalIdentity) -> IssuedGatewayInternalToken {
        self.issuer
            .issue(
                identity.profile,
                identity.server,
                identity.actor,
                identity.authority,
                identity.request_context,
                identity.expires_at,
            )
            .unwrap()
    }
}
pub fn identity(plane: &GatewayControlPlane) -> GatewayInternalIdentity {
    let scopes = KnowledgeScope::ALL
        .iter()
        .map(|s| s.name().clone())
        .chain(["operator:use".parse().unwrap()])
        .collect();
    let actor = Principal {
        id: "https://veoveo.example/oauth#operator-service"
            .parse()
            .unwrap(),
        kind: PrincipalKind::Service,
        issuer: plane.authorization_servers[0].issuer.clone(),
        subject: "operator-service".parse().unwrap(),
        tenant: Some("tenant-a".parse().unwrap()),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes,
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    };
    let context = &plane.work_contexts[0];
    let now = Utc::now();
    let expires = now + TimeDelta::minutes(3);
    GatewayInternalIdentity {
        issuer: GATEWAY_INTERNAL_TOKEN_ISSUER.parse().unwrap(),
        profile: plane.profiles[0].id.clone(),
        server: "knowledge".parse().unwrap(),
        actor: actor.clone(),
        authority: veoveo_types::InvocationAuthority {
            work_context: context.id.clone(),
            tenant: context.tenant.clone(),
            policy_revision: context.policy_revision.clone(),
            membership: WorkContextMembershipLevel::Contributor,
            output_policy: context.output_policy.clone(),
            provenance: InvocationProvenance::Automated,
        },
        request_context: Some(GatewayRequestContext {
            format: GatewayRequestContextFormat::V2,
            audit: audit::AuditRequest::background(),
            principal: actor.clone(),
            access_token: AccessTokenSubject {
                managed_execution: None,
                issuer: actor.issuer,
                subject: actor.subject,
                oauth_client_id: "operator-service".parse().unwrap(),
                session_family: None,
                audience: plane.profiles[0].protected_resource.clone(),
                work_context: context.id.clone(),
                invocation_mode: InvocationMode::Automated,
                initiator: None,
                delegation_id: None,
                scopes: actor.scopes,
                jwt_id: Some(uuid::Uuid::now_v7().to_string().parse().unwrap()),
                issued_at: now,
                not_before: None,
                expires_at: expires,
            },
        }),
        jwt_id: "unsigned-fixture".parse().unwrap(),
        issued_at: now,
        not_before: now,
        expires_at: expires,
    }
}
pub async fn directory(store: &PlatformStore, identity: &GatewayInternalIdentity) {
    for p in [
        &identity.actor,
        &identity.request_context.as_ref().unwrap().principal,
    ] {
        store
            .ensure_identity(
                identity.authority.tenant.as_str(),
                p.id.as_str(),
                p.issuer.as_str(),
                p.subject.as_str(),
                match p.kind {
                    PrincipalKind::User => veoveo_platform_store::PrincipalKind::User,
                    PrincipalKind::Service => veoveo_platform_store::PrincipalKind::Service,
                },
            )
            .await
            .unwrap();
    }
}
pub async fn revoke(store: &PlatformStore, identity: &GatewayInternalIdentity) {
    let token = &identity.request_context.as_ref().unwrap().access_token;
    let jwt = token.jwt_id.as_ref().unwrap();
    store
        .upsert_gateway_jwt_revocation(veoveo_platform_store::GatewayJwtRevocationRecord {
            id: veoveo_platform_store::gateway_jwt_revocation_record_id(
                identity.profile.as_str(),
                token.issuer.as_str(),
                jwt.as_str(),
            ),
            profile: identity.profile.to_string(),
            issuer: token.issuer.to_string(),
            jwt_id: jwt.to_string(),
            revoked_at: Utc::now(),
            expires_at: token.expires_at,
            reason: Some("fixture".into()),
            payload: OpenObject::default(),
        })
        .await
        .unwrap();
}

pub struct Server {
    pub indexing: tokio::sync::watch::Sender<veoveo_knowledge_mcp::coordinator::CoordinatorState>,
    pub base: String,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    pub async fn new<E: Embeddings + 'static>(
        store: PlatformStore,
        embeddings: Arc<E>,
        signing: &Signing,
    ) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let (indexing, ready) = tokio::sync::watch::channel(
            veoveo_knowledge_mcp::coordinator::CoordinatorState::CatalogReady,
        );
        let router = veoveo_knowledge_mcp::host::server(
            KnowledgeMcp::new(
                store,
                embeddings,
                veoveo_gateway_catalog::registry().expect("catalog recipe"),
            ),
            &veoveo_mcp_contract::PublicDeployment::new(format!("http://{address}")).unwrap(),
            true,
            vec![address.to_string()],
            signing.trust.clone(),
            veoveo_knowledge_mcp::indexing::IndexingReadiness::new(vec![ready]).unwrap(),
        )
        .unwrap()
        .into_router();
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            indexing,
            base: format!("http://{address}/knowledge"),
            stop,
            task,
        }
    }
    pub async fn sdk(
        &self,
        bearer: String,
    ) -> rmcp::service::RunningService<rmcp::RoleClient, ClientConfig> {
        let transport = StreamableHttpClientTransport::with_client(
            reqwest::Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", self.base))
                .auth_header(bearer),
        );
        tokio::time::timeout(
            Duration::from_secs(5),
            ClientConfig::new(
                ClientCapabilities::default(),
                Implementation::new("knowledge-fixture", "1"),
            )
            .serve_with_lifecycle(
                transport,
                rmcp::ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            ),
        )
        .await
        .unwrap()
        .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}
