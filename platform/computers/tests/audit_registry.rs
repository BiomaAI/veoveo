//! Computers admission requires its Audit read codec before serving any operation.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
use std::time::Duration;
use veoveo_audit_contract::AuditTargetError;
use veoveo_computers::{ComputersStore, api::ProviderInstanceId};

#[tokio::test]
async fn startup_requires_computer_codec_without_installing_its_schema() {
    let qualification = async {
        let empty = fixture::TestDb::new().await;
        assert!(
            matches!(ComputersStore::new(empty.a.clone(),ProviderInstanceId::new(),veoveo_gateway_catalog::registry().unwrap()),Err(AuditTargetError::Unbound(kind)) if kind=="computer")
        );
        let registry = veoveo_gateway_catalog::audit_target_registry().unwrap();
        let bound = fixture::TestDb::with_audit_targets(registry).await;
        assert!(
            ComputersStore::new(
                bound.a.clone(),
                ProviderInstanceId::new(),
                veoveo_gateway_catalog::registry().unwrap()
            )
            .is_ok()
        );
    };
    tokio::time::timeout(Duration::from_secs(90), qualification)
        .await
        .expect("Computers Audit startup qualification exceeded 90 seconds");
}
