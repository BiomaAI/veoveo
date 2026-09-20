//! Durable-route authorization against an isolated store and the current policy catalog.
use std::collections::BTreeSet;

use base64::Engine as _;
use chrono::{TimeDelta, Utc};
use veoveo_mcp_contract::{
    AccessTokenSubject, DataLabelId, DelegationId, GatewayAction, GatewayControlPlane,
    GatewayInternalSigningKey, InvocationMode, InvocationProvenance, OAuthClientId, PolicyVersion,
    PrincipalId, ProtectedResourceId, ScopeName, TaskExposure, TenantId, TokenIssuer,
    WorkContextId, WorkContextMembershipLevel,
};
use veoveo_platform_store::{PrincipalKind, TaskId};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime};

use super::*;
use crate::{
    GatewayCatalog,
    state::{GatewayTaskOwnership, GatewayTaskRouteDraft},
};

use crate::test_store as fixture;

pub(crate) fn subject() -> AuthenticatedSubject {
    let mut actor = super::tests::principal();
    let mut authority = super::tests::authority();
    actor.tenant = Some(TenantId::new("enterprise").unwrap());
    authority.tenant = actor.tenant.clone().unwrap();
    actor.scopes = BTreeSet::from([ScopeName::new("operator:use").unwrap()]);
    actor.data_labels.insert(DataLabelId::new("cui").unwrap());
    AuthenticatedSubject {
        access_token: AccessTokenSubject {
            managed_agent: None,
            issuer: actor.issuer.clone(),
            subject: actor.subject.clone(),
            oauth_client_id: OAuthClientId::new("console").unwrap(),
            session_family: None,
            audience: ProtectedResourceId::new("https://veoveo.example/mcp/workspace").unwrap(),
            work_context: authority.work_context.clone(),
            invocation_mode: InvocationMode::Direct,
            initiator: Some(actor.id.clone()),
            delegation_id: None,
            scopes: actor.scopes.clone(),
            jwt_id: None,
            issued_at: Utc::now(),
            not_before: None,
            expires_at: Utc::now() + TimeDelta::minutes(15),
        },
        principal: actor.clone(),
        actor,
        principal_display_name: None,
        authority,
    }
}

pub(super) fn gateway(state: GatewayState, plane: GatewayControlPlane) -> GatewayMcp {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let key = base64::engine::general_purpose::STANDARD
        .decode("MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl")
        .unwrap();
    GatewayMcp::new(
        GatewayCatalogHandle::new(Arc::new(GatewayCatalog::from_control_plane(plane).unwrap())),
        GatewayProfileId::new("workspace").unwrap(),
        state,
        GatewayInternalTokenIssuer::new(
            TokenIssuer::new("test-gateway").unwrap(),
            GatewayInternalSigningKey::new("test", key).unwrap(),
        ),
        GatewayUpstreamHttpClientPool::default(),
    )
}

fn draft(subject: &AuthenticatedSubject, source_task: Option<TaskId>) -> GatewayTaskRouteDraft {
    GatewayTaskRouteDraft {
        tenant_key: subject.authority.tenant.to_string(),
        owner_key: subject.actor.id.to_string(),
        owner_issuer: subject.actor.issuer.to_string(),
        owner_subject: subject.actor.subject.to_string(),
        owner_kind: PrincipalKind::User,
        work_context: subject.authority.work_context.to_string(),
        profile: "workspace".into(),
        server: "media".into(),
        source_task_id: source_task
            .map(|id| id.to_string())
            .unwrap_or_else(|| "external/opaque".into()),
        source_task,
        authority_digest: hex::encode(
            invocation_authorization_fingerprint(&subject.actor, &subject.authority).unwrap(),
        ),
        ownership: GatewayTaskOwnership::from_invocation(&subject.actor, &subject.authority),
        ttl_ms: Some(60_000),
    }
}

async fn admitted(gateway: &GatewayMcp, subject: &AuthenticatedSubject, task: &str) -> bool {
    for action in [
        GatewayAction::TasksGet,
        GatewayAction::TasksUpdate,
        GatewayAction::TasksCancel,
        GatewayAction::SubscriptionsListen,
    ] {
        if let Err(error) = gateway
            .authorize_canonical_task_for_subject(subject, action, task)
            .await
        {
            eprintln!("Task action {action:?}: {error}");
            return false;
        }
    }
    true
}

#[tokio::test]
async fn recovery_uses_durable_identity_and_current_permissions_for_all_task_actions() {
    let db = fixture::TestDb::new().await;
    let state = GatewayState::new(db.a.clone());
    let plane: GatewayControlPlane =
        serde_json::from_str(include_str!("../../../../configs/gateway.local.json")).unwrap();
    let gateway = gateway(state.clone(), plane.clone());
    let original = subject();
    let (id, _) = state
        .create_task_route(draft(&original, None))
        .await
        .unwrap();
    assert!(admitted(&gateway, &original, id.as_str()).await);
    let mut refreshed = original.clone();
    refreshed
        .actor
        .scopes
        .insert(ScopeName::new("time:timeline").unwrap());
    refreshed.actor.authenticated_at = Some(Utc::now());
    refreshed.authority.policy_revision = PolicyVersion::new("r2").unwrap();
    refreshed.authority.membership = WorkContextMembershipLevel::Contributor;
    refreshed.principal = refreshed.actor.clone();
    refreshed.access_token.scopes = refreshed.actor.scopes.clone();
    refreshed.access_token.issued_at = Utc::now();
    assert!(admitted(&gateway, &refreshed, id.as_str()).await);
    // Returning to the smaller but still sufficient scope set also works.
    assert!(admitted(&gateway, &original, id.as_str()).await);
    let mut denied = Vec::new();
    let mut caller = refreshed.clone();
    caller.actor.id = PrincipalId::new("other#person").unwrap();
    denied.push(caller);
    let mut caller = refreshed.clone();
    caller.actor.issuer = TokenIssuer::new("https://other.test").unwrap();
    denied.push(caller);
    let mut caller = refreshed.clone();
    caller.authority.tenant = TenantId::new("other").unwrap();
    denied.push(caller);
    let mut caller = refreshed.clone();
    caller.authority.work_context = WorkContextId::new("other").unwrap();
    denied.push(caller);
    let mut caller = refreshed.clone();
    caller.actor.data_labels.clear();
    denied.push(caller);
    let mut caller = refreshed.clone();
    caller.principal.scopes.clear();
    caller.actor.scopes.clear();
    denied.push(caller);
    let mut caller = refreshed.clone();
    caller.principal.roles.clear();
    caller.actor.roles.clear();
    denied.push(caller);
    let mut caller = refreshed.clone();
    caller.authority.provenance = InvocationProvenance::Automated;
    denied.push(caller);
    let mut caller = refreshed.clone();
    caller.authority.provenance = InvocationProvenance::Direct {
        initiator: PrincipalId::new("other#initiator").unwrap(),
    };
    denied.push(caller);
    for caller in denied {
        for action in [
            GatewayAction::TasksGet,
            GatewayAction::TasksUpdate,
            GatewayAction::TasksCancel,
            GatewayAction::SubscriptionsListen,
        ] {
            assert!(
                gateway
                    .authorize_canonical_task_for_subject(&caller, action, id.as_str())
                    .await
                    .is_err()
            );
        }
    }
    let mut other_profile = gateway.clone();
    other_profile.profile_id = GatewayProfileId::new("admin").unwrap();
    assert!(!admitted(&other_profile, &refreshed, id.as_str()).await);
    let mut revoked = plane.clone();
    for policy in &mut revoked.policies {
        policy.rules.clear();
    }
    gateway.catalog.replace(Arc::new(
        GatewayCatalog::from_control_plane(revoked).unwrap(),
    ));
    assert!(!admitted(&gateway, &refreshed, id.as_str()).await);
    let mut hidden = plane;
    for profile in &mut hidden.profiles {
        for exposure in &mut profile.servers {
            exposure.tasks = TaskExposure::Disabled;
        }
    }
    gateway.catalog.replace(Arc::new(
        GatewayCatalog::from_control_plane(hidden).unwrap(),
    ));
    assert!(!admitted(&gateway, &refreshed, id.as_str()).await);
}

#[test]
fn delegated_ownership_requires_the_same_initiator_and_grant() {
    let mut caller = subject();
    caller.authority.provenance = InvocationProvenance::Delegated {
        initiator: caller.principal.id.clone(),
        delegation_id: DelegationId::new("grant-1").unwrap(),
    };
    let owner = GatewayTaskOwnership::from_invocation(&caller.actor, &caller.authority);
    assert!(owner.allows(&caller.actor, &caller.authority));
    for provenance in [
        InvocationProvenance::Delegated {
            initiator: caller.principal.id.clone(),
            delegation_id: DelegationId::new("grant-2").unwrap(),
        },
        InvocationProvenance::Delegated {
            initiator: PrincipalId::new("other#initiator").unwrap(),
            delegation_id: DelegationId::new("grant-1").unwrap(),
        },
        InvocationProvenance::Direct {
            initiator: caller.principal.id.clone(),
        },
        InvocationProvenance::Automated,
    ] {
        caller.authority.provenance = provenance;
        assert!(!owner.allows(&caller.actor, &caller.authority));
    }
}

#[tokio::test]
async fn version_zero_shared_task_recovers_without_rewriting_or_rebinding_external_routes() {
    let db = fixture::TestDb::new().await;
    let state = GatewayState::new(db.a.clone());
    let original = subject();
    let runtime = TaskRuntime::new(db.a.clone(), "media", "ownership-fixture");
    let task_id = TaskId::new();
    let owner = TaskOwner {
        principal_key: original.actor.id.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: original.actor.issuer.to_string(),
        subject: original.actor.subject.to_string(),
        profile: "workspace".into(),
        tenant_key: Some(original.authority.tenant.to_string()),
        data_labels: original
            .actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        authority: original.authority.clone(),
    };
    runtime
        .create(CreateTask {
            task_id,
            owner,
            server: "media".into(),
            task_type: "fixture".into(),
            request: serde_json::json!({}),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(60_000),
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    let (id, before) = state
        .create_task_route(draft(&original, Some(task_id)))
        .await
        .unwrap();
    let (external, external_before) = state
        .create_task_route(draft(&original, None))
        .await
        .unwrap();
    // This is exactly the stored shape produced by pre-0081 gateway replicas.
    db.a.client()
        .query("UPDATE gateway_task_route UNSET ownership;")
        .await
        .unwrap()
        .check()
        .unwrap();
    let gateway = gateway(
        GatewayState::new(db.b.clone()),
        serde_json::from_str(include_str!("../../../../configs/gateway.local.json")).unwrap(),
    );
    let mut refreshed = original.clone();
    refreshed
        .actor
        .scopes
        .insert(ScopeName::new("time:timeline").unwrap());
    refreshed.principal = refreshed.actor.clone();
    assert!(admitted(&gateway, &refreshed, id.as_str()).await);
    assert!(admitted(&gateway, &original, external.as_str()).await);
    assert!(!admitted(&gateway, &refreshed, external.as_str()).await);
    for (key, retained) in [(&id, &before), (&external, &external_before)] {
        let after = state.task_route(key).await.unwrap().unwrap();
        assert_eq!(after.authority_digest, retained.authority_digest);
        assert_eq!(after.created_at, retained.created_at);
        assert_eq!(after.expires_at, retained.expires_at);
        assert!(after.ownership.is_none());
    }
    // A linked record from another server cannot supply ownership evidence.
    db.a.client()
        .query("UPDATE ONLY $source SET server = mcp_server:other;")
        .bind(("source", task_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(!admitted(&gateway, &refreshed, id.as_str()).await);
}
