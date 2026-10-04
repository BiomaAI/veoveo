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
    let mut builder = veoveo_mcp_gateway::auth::access_token_extension_registry_builder();
    let name =
        veoveo_types::ExtensionName::new(veoveo_agent_runtime::contract::MANAGED_AGENT_CLAIM)?;
    builder.reserve(name.clone())?;
    let key = builder.bind(
        &name,
        veoveo_agent_runtime::contract::admit_managed_agent_token,
    )?;
    let registry = builder.build();
    GatewayState::new(platform.clone())
        .bind_token_extensions(registry.clone())?
        .bind_oauth_client_resolver(Arc::new(ManagedOAuthClientResolver::new(
            platform, templates, registry, key,
        )))
}
