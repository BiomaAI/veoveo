//! Tests reuse the production registration source without a reverse recipe package edge.
#[path = "../../platform/gateway/catalog/src/lib.rs"]
mod installation_catalog;
use std::sync::OnceLock;
pub fn fresh_registry() -> veoveo_gateway_contract::CatalogRegistry {
    installation_catalog::registry().unwrap()
}
pub fn registry() -> veoveo_gateway_contract::CatalogRegistry {
    static REGISTRY: OnceLock<veoveo_gateway_contract::CatalogRegistry> = OnceLock::new();
    REGISTRY.get_or_init(fresh_registry).clone()
}
