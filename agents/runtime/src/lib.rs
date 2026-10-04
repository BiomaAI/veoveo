//! Durable scheduling and delivery state for autonomous Veoveo agents.
//!
//! The platform store is authoritative. In-process notifications and Surreal
//! LIVE streams are latency hints only; every recovery path starts from the
//! persisted pending rows.

#[cfg(test)]
extern crate self as veoveo_agent_runtime;

#[cfg(feature = "contract")]
pub mod contract;

#[cfg(feature = "catalog")]
pub mod catalog;

#[cfg(feature = "runtime")]
mod control;
#[cfg(feature = "runtime")]
mod runtime;
#[cfg(feature = "runtime")]
mod types;

#[cfg(feature = "runtime")]
pub use control::*;
#[cfg(feature = "runtime")]
pub use runtime::AgentRuntime;
#[cfg(feature = "runtime")]
pub use types::*;

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
    templates: std::sync::Arc<crate::gateway::ManagedTemplateCatalog>,
) -> anyhow::Result<veoveo_mcp_gateway::GatewayState> {
    let (registry, key) = fixture_claims();
    veoveo_mcp_gateway::GatewayState::new(store.clone())
        .bind_token_extensions(registry.clone())?
        .bind_oauth_client_resolver(std::sync::Arc::new(
            crate::gateway::ManagedOAuthClientResolver::new(store, templates, registry, key),
        ))
}

#[cfg(all(test, feature = "gateway"))]
fn fixture_claims() -> (
    veoveo_types::ExtensionRegistry,
    veoveo_types::ExtensionKey<contract::ManagedAgentToken>,
) {
    static CLAIMS: std::sync::OnceLock<(
        veoveo_types::ExtensionRegistry,
        veoveo_types::ExtensionKey<contract::ManagedAgentToken>,
    )> = std::sync::OnceLock::new();
    CLAIMS
        .get_or_init(|| {
            let mut builder =
                veoveo_types::ExtensionRegistryBuilder::new(std::iter::empty::<String>());
            let name = veoveo_types::ExtensionName::new(contract::MANAGED_AGENT_CLAIM).unwrap();
            builder.reserve(name.clone()).unwrap();
            let key = builder
                .bind(&name, contract::admit_managed_agent_token)
                .unwrap();
            (builder.build(), key)
        })
        .clone()
}

#[cfg(feature = "schema")]
pub mod observation;
#[cfg(feature = "schema")]
pub use observation::AgentObservationTable;
