//! Explicit owner admission bound before any catalog is admitted or published.
use anyhow::{Context, Result, ensure};
use veoveo_mcp_contract::GatewayControlPlane;

#[derive(Debug, Clone, Default)]
pub struct GatewayCatalogAdmission {
    registry: Option<veoveo_gateway_contract::CatalogRegistry>,
}
impl GatewayCatalogAdmission {
    pub fn unbound() -> Self {
        Self::default()
    }
    pub fn bind(mut self, registry: veoveo_gateway_contract::CatalogRegistry) -> Result<Self> {
        ensure!(
            self.registry.is_none(),
            "gateway catalog admission already bound"
        );
        self.registry = Some(registry);
        Ok(self)
    }
    pub fn ensure_bound(&self) -> Result<()> {
        self.registry().map(|_| ())
    }
    pub fn registry(&self) -> Result<&veoveo_gateway_contract::CatalogRegistry> {
        self.registry
            .as_ref()
            .context("gateway catalog admission is unbound; bind the installation catalog registry")
    }
    pub fn validate(
        &self,
        control_plane: &GatewayControlPlane,
    ) -> Result<veoveo_gateway_contract::AdmittedCatalogSections> {
        Ok(control_plane.validate(self.registry()?)?)
    }
    pub(crate) fn same_binding(&self, other: &Self) -> bool {
        matches!((&self.registry,&other.registry),(Some(a),Some(b)) if a.same_binding(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GatewayCatalog, GatewayCatalogHandle};
    use std::sync::Arc;
    #[test]
    fn admission_is_required_unique_and_retained_on_reload() {
        let plane: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
        assert!(
            GatewayCatalog::from_control_plane(plane.clone(), GatewayCatalogAdmission::unbound())
                .is_err()
        );
        let registry = crate::catalog_fixture::registry();
        let admission = GatewayCatalogAdmission::unbound()
            .bind(registry.clone())
            .unwrap();
        assert!(admission.clone().bind(registry).is_err());
        let catalog = GatewayCatalog::from_control_plane(plane.clone(), admission.clone()).unwrap();
        let handle = GatewayCatalogHandle::new(Arc::new(catalog));
        let mut changes = handle.subscribe();
        handle
            .replace(Arc::new(
                GatewayCatalog::from_control_plane(plane.clone(), admission).unwrap(),
            ))
            .unwrap();
        assert_eq!(handle.snapshot().generation(), 1);
        assert_eq!(*changes.borrow_and_update(), 1);
        let foreign = GatewayCatalogAdmission::unbound()
            .bind(crate::catalog_fixture::fresh_registry())
            .unwrap();
        assert!(
            handle
                .replace(Arc::new(
                    GatewayCatalog::from_control_plane(plane, foreign).unwrap()
                ))
                .is_err()
        );
        assert_eq!(handle.snapshot().generation(), 1);
        assert!(!changes.has_changed().unwrap());
    }
}
