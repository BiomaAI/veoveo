#[path = "../../../testing/fixtures/catalog_admission.rs"]
mod catalog_admission;
use std::collections::BTreeSet;

use chrono::Utc;
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayControlPlane, GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
    GatewayControlPlaneRevisionSource, OAuthClientId, PolicySet, TenantDefinition,
    WorkContextDefinition, WorkContextMembershipRule,
};
use veoveo_mcp_gateway::GatewayControlStore;
use veoveo_platform_store::{StoreConfig, StoreCredentials, deterministic_tenant_id};
use veoveo_types::{AccessSubject, GroupId, PolicyVersion, PrincipalId, TenantId, WorkContextId};
use veoveo_types::{WorkContextMembershipLevel, WorkContextOutputPolicy};

#[tokio::test]
async fn publishes_immutable_revisions_and_moves_active_pointer_atomically() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint = std::env::var("VEOVEO_SURREAL_ENDPOINT")
        .unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let namespace = std::env::var("VEOVEO_SURREAL_NAMESPACE")
        .unwrap_or_else(|_| "veoveo_integration".to_owned());
    let database_prefix =
        std::env::var("VEOVEO_SURREAL_DATABASE").unwrap_or_else(|_| "platform_test".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USERNAME").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let database = format!("{database_prefix}_{}", Uuid::new_v4().simple());
    let config = StoreConfig::builder(
        endpoint,
        namespace,
        database,
        StoreCredentials::root(username, password),
    )
    .build()
    .unwrap();
    let store = GatewayControlStore::connect(config, catalog_admission::binding())
        .await
        .unwrap();

    assert!(store.load_active_revision().await.unwrap().is_none());
    assert!(store.load_active_revision_head().await.unwrap().is_none());

    let audit_context = veoveo_audit_contract::AuditContext {
        actor: veoveo_audit_contract::AuditActor {
            principal: PrincipalId::parse("integration-admin").unwrap(),
            kind: veoveo_audit_contract::AuditPrincipalKind::Service,
            tenant: None,
            oauth_client: None,
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        },
        authority: Default::default(),
        request: veoveo_audit_contract::AuditRequest::background(),
    };
    let first = revision("gcp-first", "a".repeat(64), empty_control_plane());
    store.record_revision(&first, &audit_context).await.unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(first.clone())
    );
    let first_head = store.load_active_revision_head().await.unwrap().unwrap();
    assert_eq!(first_head.revision_id, first.revision_id);
    assert_eq!(first_head.sha256, first.sha256);
    assert_eq!(store.revision_count().await.unwrap(), 1);
    assert_eq!(store.object_count_for_active_revision().await.unwrap(), 0);

    let mut second_plane = empty_control_plane();
    second_plane.tenants.push(TenantDefinition {
        id: TenantId::parse("tenant-integration").unwrap(),
        title: Some("Integration tenant".to_owned()),
        description: None,
        metadata: serde_json::json!({}),
    });
    second_plane.policies.push(PolicySet {
        version: PolicyVersion::parse("r1").unwrap(),
        rules: Vec::new(),
        metadata: serde_json::Value::Null,
    });
    second_plane.work_contexts.push(WorkContextDefinition {
        id: WorkContextId::parse("operations").unwrap(),
        tenant: TenantId::parse("tenant-integration").unwrap(),
        title: "Operations".to_owned(),
        policy_revision: PolicyVersion::parse("r1").unwrap(),
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Group(GroupId::parse("operations").unwrap()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        memberships: vec![WorkContextMembershipRule {
            level: WorkContextMembershipLevel::Contributor,
            principals: BTreeSet::new(),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            oauth_clients: BTreeSet::from([OAuthClientId::parse("automation").unwrap()]),
        }],
    });
    let second = revision("gcp-second", "b".repeat(64), second_plane);
    store
        .record_revision(&second, &audit_context)
        .await
        .unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(second.clone())
    );
    let second_head = store.load_active_revision_head().await.unwrap().unwrap();
    assert_eq!(second_head.revision_id, second.revision_id);
    assert_eq!(second_head.sha256, second.sha256);
    assert_eq!(store.revision_count().await.unwrap(), 2);
    assert_eq!(store.object_count_for_active_revision().await.unwrap(), 3);
    let tenant_id = deterministic_tenant_id("tenant-integration").unwrap();
    let context = store
        .platform_store()
        .work_context_by_key(tenant_id, "operations")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(context.title, "Operations");
    assert_eq!(context.output_policy.owner_key, "operations");
    assert_eq!(
        context.membership_for_oauth_client("automation"),
        Some(veoveo_platform_store::WorkContextMembershipLevel::Contributor)
    );

    let mut third_plane = second.control_plane.clone();
    third_plane.work_contexts.clear();
    let third = revision("gcp-third", "c".repeat(64), third_plane);
    store.record_revision(&third, &audit_context).await.unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(third.clone())
    );
    assert_eq!(store.revision_count().await.unwrap(), 3);
    assert_eq!(store.object_count_for_active_revision().await.unwrap(), 2);
    assert!(
        store
            .platform_store()
            .work_context_by_key(tenant_id, "operations")
            .await
            .unwrap()
            .is_none()
    );

    assert!(
        store
            .record_revision(&second, &audit_context)
            .await
            .is_err(),
        "duplicate immutable revision unexpectedly succeeded"
    );
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(third.clone()),
        "failed publication moved the active pointer"
    );
    assert_eq!(store.revision_count().await.unwrap(), 3);

    let activities: Vec<String> = store
        .platform_store()
        .client()
        .query(
            include_str!("queries/control_store/publishes_immutable_revisions_and_moves_active_pointer_atomically/statement_1.surql"),
        )
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap();
    assert_eq!(
        activities,
        ["account_create", "account_delete"],
        "failed re-publication must not recreate or re-audit a deleted Work Context"
    );
}

fn revision(
    id: &str,
    sha256: String,
    control_plane: GatewayControlPlane,
) -> GatewayControlPlaneRevision {
    GatewayControlPlaneRevision {
        revision_id: GatewayControlPlaneRevisionId::parse(id).unwrap(),
        sha256,
        source: GatewayControlPlaneRevisionSource::SeedFile,
        applied_at: Utc::now(),
        applied_by: PrincipalId::parse("integration-admin").unwrap(),
        tenant: None,
        control_plane,
    }
}

fn empty_control_plane() -> GatewayControlPlane {
    GatewayControlPlane {
        branding: None,
        identity_providers: Vec::new(),
        authorization_servers: Vec::new(),
        servers: Vec::new(),
        profiles: Vec::new(),
        extensions: Default::default(),
        tenants: Vec::new(),
        work_contexts: Vec::new(),
        policies: Vec::new(),
        data_labels: Vec::new(),
        oauth_clients: Vec::new(),
        oidc_clients: Vec::new(),
        secrets: Vec::new(),
        metadata: serde_json::json!({}),
    }
}
