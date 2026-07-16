use chrono::Utc;
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayControlPlane, GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
    GatewayControlPlaneRevisionSource, PrincipalId, TenantDefinition, TenantId,
};
use veoveo_mcp_gateway::GatewayControlStore;
use veoveo_platform_store::{StoreConfig, StoreCredentials};

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
    .migrate_on_connect(true)
    .build()
    .unwrap();
    let store = GatewayControlStore::connect(config).await.unwrap();

    assert!(store.load_active_revision().await.unwrap().is_none());

    let first = revision("gcp-first", "a".repeat(64), empty_control_plane());
    store.record_revision(&first).await.unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(first.clone())
    );
    assert_eq!(store.revision_count().await.unwrap(), 1);
    assert_eq!(store.object_count_for_active_revision().await.unwrap(), 0);

    let mut second_plane = empty_control_plane();
    second_plane.tenants.push(TenantDefinition {
        id: TenantId::new("tenant-integration").unwrap(),
        title: Some("Integration tenant".to_owned()),
        description: None,
        metadata: serde_json::json!({}),
    });
    let second = revision("gcp-second", "b".repeat(64), second_plane);
    store.record_revision(&second).await.unwrap();
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(second.clone())
    );
    assert_eq!(store.revision_count().await.unwrap(), 2);
    assert_eq!(store.object_count_for_active_revision().await.unwrap(), 1);

    assert!(
        store.record_revision(&first).await.is_err(),
        "duplicate immutable revision unexpectedly succeeded"
    );
    assert_eq!(
        store.load_active_revision().await.unwrap(),
        Some(second.clone()),
        "failed publication moved the active pointer"
    );
    assert_eq!(store.revision_count().await.unwrap(), 2);

    let outbox = store.platform_store().read_outbox(0, 10).await.unwrap();
    assert_eq!(outbox.events.len(), 2);
    assert_eq!(
        outbox.events.last().unwrap().aggregate_id,
        second.revision_id.as_str()
    );
}

fn revision(
    id: &str,
    sha256: String,
    control_plane: GatewayControlPlane,
) -> GatewayControlPlaneRevision {
    GatewayControlPlaneRevision {
        revision_id: GatewayControlPlaneRevisionId::new(id).unwrap(),
        sha256,
        source: GatewayControlPlaneRevisionSource::SeedFile,
        applied_at: Utc::now(),
        applied_by: PrincipalId::new("integration-admin").unwrap(),
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
        recording_ingest_resources: Vec::new(),
        tenants: Vec::new(),
        policies: Vec::new(),
        data_labels: Vec::new(),
        oauth_clients: Vec::new(),
        oidc_clients: Vec::new(),
        secrets: Vec::new(),
        metadata: serde_json::json!({}),
    }
}
