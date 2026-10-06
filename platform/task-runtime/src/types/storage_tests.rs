use super::*;
use veoveo_platform_store::{TaskOwnerRecord, TaskRequestRecord};

fn record() -> TaskRecord {
    let owner: TaskOwner = serde_json::from_value(serde_json::json!({
        "principal_key":"pilot", "principal_kind":"service", "issuer":"https://storage.invalid",
        "subject":"pilot", "profile":"operator", "tenant_key":"test", "data_labels":["clearance"],
        "authority":{"work_context":"mission","tenant":"test","membership":"owner","policy_revision":"native",
        "output_policy":{"owner":{"kind":"principal","id":"pilot"},"data_labels":["output"]},"provenance":{"mode":"automated"}}
    })).unwrap();
    let now = Utc::now();
    TaskRecord {
        id: veoveo_platform_store::task_record_id(TaskId::new()),
        tenant: deterministic_tenant_id(owner.tenant_key())
            .unwrap()
            .record_id(),
        owner: deterministic_principal_id(owner.tenant_key(), &owner.principal_key)
            .unwrap()
            .record_id(),
        work_context: deterministic_work_context_id(
            owner.tenant_key(),
            owner.authority.work_context.as_str(),
        )
        .unwrap()
        .record_id(),
        initiator: None,
        invocation_mode: veoveo_platform_store::InvocationMode::Automated,
        delegation_id: None,
        policy_revision: owner.authority.policy_revision.to_string(),
        authority: crate::runtime::authority_record(&owner.authority),
        profile: RecordId::new("profile", owner.profile.clone()),
        server: RecordId::new("mcp_server", "native"),
        task_type: "native".parse().unwrap(),
        status: StoreTaskStatus::Queued,
        recovery_class: RecoveryClass::Resume,
        request: TaskRequestRecord {
            input: serde_json::json!(null),
            status_message: None,
            ttl_ms: None,
            poll_interval_ms: Some(u64::MAX),
        },
        owner_context: TaskOwnerRecord::try_from(&owner).unwrap(),
        progress: 0.0,
        result: None,
        result_uri: None,
        error: None,
        result_artifact: None,
        idempotency_key: None,
        lease_owner: None,
        lease_expires_at: None,
        cancel_requested_at: None,
        created_at: now,
        created_at_exact: veoveo_platform_store::TaskTimestampToken::new(now),
        updated_at: now,
        updated_at_exact: veoveo_platform_store::TaskTimestampToken::new(now),
        started_at: None,
        completed_at: None,
        retention_expires_at: None,
        retention_pins: vec![],
        search_text: String::new(),
    }
}

#[test]
fn driver_snapshot_rejects_wrong_tables_and_independent_authority_disagreement() {
    let valid = record();
    assert!(record_to_snapshot(valid.clone()).is_ok());
    for field in ["server", "profile", "owner", "tenant", "context"] {
        let mut invalid = valid.clone();
        let reference = match field {
            "server" => &mut invalid.server,
            "profile" => &mut invalid.profile,
            "owner" => &mut invalid.owner,
            "tenant" => &mut invalid.tenant,
            "context" => &mut invalid.work_context,
            _ => unreachable!(),
        };
        *reference = RecordId::new("wrong_table", reference.key.clone());
        assert!(
            matches!(
                record_to_snapshot(invalid),
                Err(TaskError::InvalidRecord(_))
            ),
            "wrong-table {field} admitted"
        );
    }
    let mut invalid = valid.clone();
    invalid.created_at_exact = veoveo_platform_store::TaskTimestampToken::new(
        valid.created_at + chrono::TimeDelta::nanoseconds(1),
    );
    assert!(matches!(
        record_to_snapshot(invalid),
        Err(TaskError::InvalidRecord(_))
    ));
    let mut invalid = valid.clone();
    invalid.updated_at_exact = veoveo_platform_store::TaskTimestampToken::new(
        valid.updated_at + chrono::TimeDelta::nanoseconds(1),
    );
    assert!(matches!(
        record_to_snapshot(invalid),
        Err(TaskError::InvalidRecord(_))
    ));
    let mut invalid = valid;
    invalid.owner_context.authority.tenant = "other".parse().unwrap();
    assert!(matches!(
        record_to_snapshot(invalid),
        Err(TaskError::InvalidRecord(_))
    ));
}

#[test]
fn snapshot_json_admits_product_only_with_successful_retained_result() {
    let mut record = record();
    record.status = StoreTaskStatus::Succeeded;
    record.result = Some(veoveo_platform_store::TaskResultRecord::new(
        serde_json::json!({"ok": true}),
    ));
    record.result_uri = Some(veoveo_types::ResourceUri::new("fixture://items/1").unwrap());
    let snapshot = record_to_snapshot(record).unwrap();
    let wire = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(
        serde_json::from_value::<TaskSnapshot>(wire.clone()).unwrap(),
        snapshot
    );

    for status in [
        "queued",
        "running",
        "waiting",
        "cancel_requested",
        "failed",
        "cancelled",
    ] {
        let mut invalid = wire.clone();
        invalid["status"] = serde_json::json!(status);
        assert!(
            serde_json::from_value::<TaskSnapshot>(invalid).is_err(),
            "{status} admitted a product address"
        );
    }
    let mut absent = wire.clone();
    absent.as_object_mut().unwrap().remove("result");
    assert!(serde_json::from_value::<TaskSnapshot>(absent).is_err());
    for address in [serde_json::json!("invalid URI"), serde_json::json!(42)] {
        let mut invalid = wire.clone();
        invalid["result_uri"] = address;
        assert!(serde_json::from_value::<TaskSnapshot>(invalid).is_err());
    }

    // Present null is a retained generic core result, distinct from absence.
    let mut retained_null = wire.clone();
    retained_null["result"] = serde_json::Value::Null;
    let decoded: TaskSnapshot = serde_json::from_value(retained_null.clone()).unwrap();
    assert_eq!(decoded.result, Some(serde_json::Value::Null));
    assert!(decoded.result_uri.is_some());
    assert_eq!(serde_json::to_value(decoded).unwrap(), retained_null);

    let mut no_product = wire;
    no_product["status"] = serde_json::json!("running");
    no_product.as_object_mut().unwrap().remove("result");
    no_product.as_object_mut().unwrap().remove("result_uri");
    let decoded: TaskSnapshot = serde_json::from_value(no_product).unwrap();
    assert!(decoded.result.is_none() && decoded.result_uri.is_none());
}
