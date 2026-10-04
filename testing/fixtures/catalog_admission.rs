//! Explicit synthetic admission for kernel-policy fixtures; owner scope checks
//! are qualified by the owning module's adapter harness.
use std::sync::{Arc, OnceLock};
use veoveo_mcp_gateway::{CatalogAdmission, GatewayCatalogAdmission};
#[derive(Debug)]
struct FixtureAdmission;
impl CatalogAdmission for FixtureAdmission {
    fn validate(&self, _: &veoveo_mcp_contract::GatewayControlPlane) -> anyhow::Result<()> {
        Ok(())
    }
}
pub fn binding() -> GatewayCatalogAdmission {
    static BINDING: OnceLock<GatewayCatalogAdmission> = OnceLock::new();
    BINDING
        .get_or_init(|| {
            GatewayCatalogAdmission::unbound()
                .bind(Arc::new(FixtureAdmission))
                .unwrap()
        })
        .clone()
}
