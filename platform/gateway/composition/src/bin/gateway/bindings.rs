//! Concrete owner adapters selected by the gateway composition.
use std::sync::Arc;
use veoveo_agent_runtime::gateway::{ManagedOAuthClientResolver, ManagedTemplateCatalog};
use veoveo_mcp_gateway::{GatewayCatalogAdmission, GatewayState};
use veoveo_platform_store::PlatformStore;

pub(super) fn catalog_admission() -> anyhow::Result<GatewayCatalogAdmission> {
    GatewayCatalogAdmission::unbound().bind(Arc::new(
        veoveo_recording_mcp::gateway::RecordingCatalogAdmission,
    ))
}

pub(super) fn gateway_state(
    platform: PlatformStore,
    templates: Arc<ManagedTemplateCatalog>,
) -> anyhow::Result<GatewayState> {
    GatewayState::new(platform.clone()).bind_oauth_client_resolver(Arc::new(
        ManagedOAuthClientResolver::new(platform, templates),
    ))
}
