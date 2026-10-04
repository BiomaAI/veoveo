//! Reuse the actual transport-free recipe; owner unit tests alias their own crate
//! so vocabulary TypeIds refer to the tested library rather than a second copy.
#[path = "../../platform/gateway/catalog/src/lib.rs"]
mod installation_catalog;
use std::sync::OnceLock;
use veoveo_mcp_gateway::GatewayCatalogAdmission;
pub fn fresh_registry() -> veoveo_gateway_contract::CatalogRegistry {
    installation_catalog::registry().unwrap()
}
pub fn registry() -> veoveo_gateway_contract::CatalogRegistry {
    static REGISTRY: OnceLock<veoveo_gateway_contract::CatalogRegistry> = OnceLock::new();
    REGISTRY.get_or_init(fresh_registry).clone()
}
pub fn binding() -> GatewayCatalogAdmission {
    static BINDING: OnceLock<GatewayCatalogAdmission> = OnceLock::new();
    BINDING
        .get_or_init(|| GatewayCatalogAdmission::unbound().bind(registry()).unwrap())
        .clone()
}
