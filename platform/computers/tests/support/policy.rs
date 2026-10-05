//! Isolated installation policy; this does not register a live MCP server.
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::GatewayControlPlane;
use veoveo_platform_store::{
    GatewayControlRevisionContent, GatewayControlRevisionSource, OpenObject, PlatformStore,
    RecordId,
};

pub fn control() -> GatewayControlPlane {
    let control: GatewayControlPlane = serde_json::from_str(include_str!("gateway.json")).unwrap();
    control
        .validate(&veoveo_gateway_catalog::registry().unwrap())
        .unwrap();
    control
}

pub async fn install(store: &PlatformStore, control: GatewayControlPlane) -> String {
    control
        .validate(&veoveo_gateway_catalog::registry().unwrap())
        .unwrap();
    let name = uuid::Uuid::now_v7().to_string();
    let content = GatewayControlRevisionContent {
        revision_id: name.clone(),
        sha256: hex::encode(Sha256::digest(serde_json::to_vec(&control).unwrap())),
        source: GatewayControlRevisionSource::SeedFile,
        applied_at: chrono::Utc::now(),
        applied_by: "isolated-computers-fixture".into(),
        tenant: None,
        control_plane: serde_json::from_value::<OpenObject>(serde_json::to_value(control).unwrap())
            .unwrap(),
    };
    store
        .client()
        .query(include_str!(
            "../queries/support/policy/install/statement_1.surql"
        ))
        .bind((
            "revision",
            RecordId::new("gateway_control_revision", name.clone()),
        ))
        .bind(("name", name.clone()))
        .bind(("content", content))
        .await
        .unwrap()
        .check()
        .unwrap();
    name
}

pub async fn install_default(store: &PlatformStore) {
    install(store, control()).await;
}
