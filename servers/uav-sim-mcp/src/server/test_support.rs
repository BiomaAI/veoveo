use chrono::{TimeDelta, Utc};
use std::collections::BTreeSet;
use veoveo_gateway_contract::ProtectedResourceId;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, GatewayProfileId, JwtId, Principal, PrincipalKind, ServerSlug,
    TokenIssuer, TokenSubject,
};
use veoveo_types::{
    AccessSubject, DataLabelId, InvocationProvenance, PolicyVersion, PrincipalId, TenantId,
    WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};
pub(super) fn identity(
    tenant: &str,
    context: &str,
    name: &str,
    labels: &[&str],
) -> GatewayInternalIdentity {
    let principal = PrincipalId::parse(name).unwrap();
    let tenant = TenantId::parse(tenant).unwrap();
    let now = Utc::now();
    let mut identity = GatewayInternalIdentity {
        issuer: TokenIssuer::parse("https://gateway.example").unwrap(),
        profile: GatewayProfileId::parse("uav-index-test").unwrap(),
        server: ServerSlug::parse("uav-sim").unwrap(),
        actor: Principal {
            id: principal.clone(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("https://identity.example").unwrap(),
            subject: TokenSubject::parse(name).unwrap(),
            tenant: Some(tenant.clone()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::from([crate::contract::UavScope::Control.into()]),
            assurances: BTreeSet::new(),
            authenticated_at: None,
            data_labels: labels
                .iter()
                .map(|label| DataLabelId::parse(*label).unwrap())
                .collect(),
        },
        authority: InvocationAuthority {
            work_context: WorkContextId::parse(context).unwrap(),
            tenant,
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: principal,
            },
        },
        request_context: None,
        jwt_id: JwtId::parse(uuid::Uuid::now_v7().to_string()).unwrap(),
        issued_at: now,
        not_before: now,
        expires_at: now + TimeDelta::minutes(5),
    };
    bind_request_context(&mut identity);
    identity
}

#[path = "../../../../testing/fixtures/store.rs"]
pub(super) mod fixture;

/// Native protocol fixtures share the hosted composition without simulator startup.
pub(super) fn state(
    store: &veoveo_platform_store::PlatformStore,
    adapter: std::sync::Arc<crate::adapter::Adapter>,
    worker: &str,
) -> std::sync::Arc<super::state::AppState> {
    use super::control_authority::VehicleControlAuthority;
    use crate::contract::SessionId;
    use std::{sync::Arc, time::Duration};
    use veoveo_mcp_contract::SubscriptionHub;
    use veoveo_task_runtime::TaskRuntime;
    let audit = super::live_view_audit::LiveViewAudit::new(store.clone());
    let live_views = super::live_view::LiveViewService::new(
        adapter.clone(),
        audit.clone(),
        super::live_view::LiveViewConfig {
            session_duration: Duration::from_secs(30),
            public_stream_url: "wss://example.test/uav-sim/live".into(),
            maximum_frame_age_ms: 1000,
        },
    )
    .unwrap();
    Arc::new(super::state::AppState {
        session_id: SessionId::parse("native-session").unwrap(),
        adapter,
        tasks: TaskRuntime::new(store.clone(), "uav-sim", worker),
        control_authority: VehicleControlAuthority::new(store.clone()),
        subscribers: Arc::new(SubscriptionHub::new()),
        live_views,
        live_view_audit: audit,
        live_view_connect_origin: "wss://example.test".into(),
    })
}

pub(super) fn context(
    peer: &rmcp::service::Peer<rmcp::RoleServer>,
    scopes: &[crate::contract::UavScope],
    tasks: bool,
) -> rmcp::service::RequestContext<rmcp::RoleServer> {
    use rmcp::{model::ClientCapabilities, service::RequestContext};
    use veoveo_mcp_contract::hosting::ForwardedBearer;
    use veoveo_types::ScopeName;
    let mut identity = identity("scope-test", "operations", "pilot", &[]);
    identity.actor.scopes = scopes.iter().copied().map(Into::into).collect();
    identity
        .actor
        .scopes
        .insert(ScopeName::parse("external:custom").unwrap());
    bind_request_context(&mut identity);
    let (mut parts, _) = axum::http::Request::new(()).into_parts();
    parts.extensions.insert(identity);
    parts
        .extensions
        .insert(ForwardedBearer::new("native-fixture"));
    let mut context = RequestContext::new(rmcp::model::NumberOrString::Number(1), peer.clone());
    context.extensions.insert(parts);
    if tasks {
        context
            .meta
            .set_client_capabilities(ClientCapabilities::builder().enable_tasks().build());
    }
    context
}

fn bind_request_context(identity: &mut GatewayInternalIdentity) {
    use veoveo_mcp_contract::{AccessTokenSubject, GatewayRequestContext, OAuthClientId};
    identity.request_context = Some(GatewayRequestContext {
        format: veoveo_mcp_contract::GatewayRequestContextFormat::V2,
        audit: veoveo_mcp_contract::audit::AuditRequest::background(),
        principal: identity.actor.clone(),
        access_token: AccessTokenSubject {
            managed_execution: None,
            issuer: identity.actor.issuer.clone(),
            subject: identity.actor.subject.clone(),
            oauth_client_id: OAuthClientId::parse("native-test").unwrap(),
            session_family: None,
            audience: ProtectedResourceId::parse("https://gateway.example/mcp/uav-index-test")
                .unwrap(),
            work_context: identity.authority.work_context.clone(),
            invocation_mode: veoveo_types::InvocationMode::Direct,
            initiator: Some(identity.actor.id.clone()),
            delegation_id: None,
            scopes: identity.actor.scopes.clone(),
            jwt_id: Some(identity.jwt_id.clone()),
            issued_at: identity.issued_at,
            not_before: Some(identity.not_before),
            expires_at: identity.expires_at,
        },
    });
}

/// Native fixtures compose the UAV and authoring dependencies used by mission admission.
pub(super) async fn database(backend: fixture::StoreBackend) -> fixture::TestDb {
    fixture::TestDb::with_backend_and_modules(
        backend,
        vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
            crate::schema::module_setup(fixture::module_lanes::execution("uav").unwrap()).unwrap(),
            veoveo_map_mcp::schema::module_setup(fixture::module_lanes::execution("map").unwrap())
                .unwrap(),
            veoveo_recording_store::schema::module_setup(
                fixture::module_lanes::execution("recordings").unwrap(),
            )
            .unwrap(),
        ],
    )
    .await
}
