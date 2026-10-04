//! Workspace persistence ownership, separate from the browser application.
#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "gateway")]
pub mod gateway;

#[cfg(all(test, feature = "gateway"))]
#[path = "../../../testing/fixtures/agent_catalog.rs"]
mod agent_catalog;
#[cfg(all(test, feature = "gateway"))]
#[path = "../../../testing/fixtures/catalog_admission.rs"]
mod catalog_fixture;
#[cfg(all(test, feature = "gateway"))]
#[path = "../../../testing/fixtures/store.rs"]
mod test_store;
#[cfg(all(test, feature = "gateway"))]
#[path = "../../../testing/fixtures/work_context_authority.rs"]
mod work_context_authority;

#[cfg(all(test, feature = "gateway"))]
fn gateway_test_state(
    store: veoveo_platform_store::PlatformStore,
    templates: std::sync::Arc<veoveo_agent_runtime::gateway::ManagedTemplateCatalog>,
) -> anyhow::Result<veoveo_mcp_gateway::GatewayState> {
    let mut builder = veoveo_mcp_gateway::auth::access_token_extension_registry_builder();
    let name =
        veoveo_types::ExtensionName::new(veoveo_agent_runtime::contract::MANAGED_AGENT_CLAIM)?;
    builder.reserve(name.clone())?;
    let key = builder.bind(
        &name,
        veoveo_agent_runtime::contract::admit_managed_agent_token,
    )?;
    let registry = builder.build();
    veoveo_mcp_gateway::GatewayState::new(store.clone())
        .bind_token_extensions(registry.clone())?
        .bind_oauth_client_resolver(std::sync::Arc::new(
            veoveo_agent_runtime::gateway::ManagedOAuthClientResolver::new(
                store, templates, registry, key,
            ),
        ))
}
