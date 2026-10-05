//! Computer owner registration preserves the frozen signed record object.
use chrono::Utc;
use veoveo_audit::{
    integrity::{canonical_bytes, record_root},
    *,
};
use veoveo_computers_contract::{ComputerAuditTarget, ComputerId, register_audit_target};
#[test]
fn owner_target_keeps_canonical_record_bytes_and_merkle_leaf() {
    let mut builder = AuditTargetRegistry::builder();
    register_audit_target(&mut builder).unwrap();
    let registry = builder.build();
    let computer = ComputerId::parse("019f25e8-cf39-7000-8000-000000000001").unwrap();
    let target = registry.target(ComputerAuditTarget { computer }).unwrap();
    let record = AuditRecord {
        draft: AuditDraft::builder(
            AuditRequest::background(),
            target,
            AuditDetail::Computer {
                activity: ComputerActivity::Create,
                stage: ComputerAuditStage::Reserved,
                task: None,
            },
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )
        .build()
        .unwrap(),
        recorded_at: Utc::now(),
    };
    let mut frozen = serde_json::to_value(&record).unwrap();
    frozen["draft"]["target"] =
        serde_json::json!({"kind":"computer","computer":"019f25e8-cf39-7000-8000-000000000001"});
    assert_eq!(
        canonical_bytes(&record).unwrap(),
        canonical_bytes(&frozen).unwrap()
    );
    let decoded: AuditRecord = registry.decoder().from_value(frozen).unwrap();
    assert_eq!(record, decoded);
    assert_eq!(
        record_root(&[record]).unwrap(),
        record_root(&[decoded]).unwrap()
    );
}

#[test]
fn literal_frozen_computer_record_hash() {
    let mut builder = AuditTargetRegistry::builder();
    register_audit_target(&mut builder).unwrap();
    let registry = builder.build();
    let record: AuditRecord = registry
        .decoder()
        .from_str(include_str!("../testdata/computer-record-v1.json"))
        .unwrap();
    assert_eq!(
        record_root(&[record]).unwrap().hex(),
        "921af6737d79acf02680f86f105ad81a68c7804161cb28d3be9a8bc15244cd9b"
    );
}
