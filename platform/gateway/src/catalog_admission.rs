//! Explicit owner admission bound before any catalog is admitted or published.
use anyhow::{Context, Result, ensure};
use std::{fmt::Debug, sync::Arc};
use veoveo_mcp_contract::GatewayControlPlane;

pub trait CatalogAdmission: Debug + Send + Sync {
    fn validate(&self, control_plane: &GatewayControlPlane) -> Result<()>;
}

#[derive(Debug, Clone, Default)]
pub struct GatewayCatalogAdmission {
    adapter: Option<Arc<dyn CatalogAdmission>>,
}
impl GatewayCatalogAdmission {
    pub fn unbound() -> Self {
        Self::default()
    }
    pub fn bind(mut self, adapter: Arc<dyn CatalogAdmission>) -> Result<Self> {
        ensure!(
            self.adapter.is_none(),
            "gateway catalog admission already bound"
        );
        self.adapter = Some(adapter);
        Ok(self)
    }
    pub fn ensure_bound(&self) -> Result<()> {
        ensure!(
            self.adapter.is_some(),
            "gateway catalog admission is unbound; bind the installation catalog adapter"
        );
        Ok(())
    }
    pub fn validate(&self, control_plane: &GatewayControlPlane) -> Result<()> {
        self.ensure_bound()?;
        control_plane.validate()?;
        self.adapter
            .as_ref()
            .context("gateway catalog admission is unbound; bind the installation catalog adapter")?
            .validate(control_plane)
    }
    pub(crate) fn same_binding(&self, other: &Self) -> bool {
        matches!((&self.adapter, &other.adapter), (Some(a), Some(b)) if Arc::ptr_eq(a, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GatewayCatalog, GatewayCatalogHandle};
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(Debug, Default)]
    struct CountAdmission(AtomicUsize);
    impl CatalogAdmission for CountAdmission {
        fn validate(&self, _: &GatewayControlPlane) -> Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    #[test]
    fn admission_is_required_unique_and_retained_on_reload() {
        let plane: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
        assert!(
            GatewayCatalog::from_control_plane(plane.clone(), GatewayCatalogAdmission::unbound())
                .is_err()
        );
        let adapter = Arc::new(CountAdmission::default());
        let admission = GatewayCatalogAdmission::unbound()
            .bind(adapter.clone())
            .unwrap();
        assert!(
            admission
                .clone()
                .bind(Arc::new(CountAdmission::default()))
                .is_err()
        );
        let catalog = GatewayCatalog::from_control_plane(plane.clone(), admission.clone()).unwrap();
        let handle = GatewayCatalogHandle::new(Arc::new(catalog));
        let mut changes = handle.subscribe();
        let replacement = GatewayCatalog::from_control_plane(plane.clone(), admission).unwrap();
        handle.replace(Arc::new(replacement)).unwrap();
        assert_eq!(adapter.0.load(Ordering::SeqCst), 2);
        assert_eq!(handle.snapshot().generation(), 1);
        assert!(*changes.borrow_and_update() == 1);
        let foreign = GatewayCatalog::from_control_plane(
            plane,
            GatewayCatalogAdmission::unbound()
                .bind(Arc::new(CountAdmission::default()))
                .unwrap(),
        )
        .unwrap();
        assert!(handle.replace(Arc::new(foreign)).is_err());
        assert_eq!(handle.snapshot().generation(), 1);
        assert!(
            !changes.has_changed().unwrap(),
            "refused reload emitted a change"
        );
    }
}
