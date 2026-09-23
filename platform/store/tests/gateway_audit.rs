#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use veoveo_platform_store::{
    AuditEventRecord, AuditOutcome, GatewayAuditKind, OpenObject, RecordId,
};

fn record() -> AuditEventRecord {
    AuditEventRecord {
        id: RecordId::new(
            "audit_event",
            surrealdb::types::Uuid::from(uuid::Uuid::now_v7()),
        ),
        tenant: None,
        actor: None,
        action: "resources_list".into(),
        resource_type: "gateway_policy".into(),
        resource_id: None,
        outcome: AuditOutcome::Allowed,
        request_id: None,
        trace_id: None,
        source_ip: None,
        details: OpenObject::default(),
        occurred_at: chrono::Utc::now(),
        search_text: String::new(),
    }
}

#[tokio::test]
async fn discovery_audit_batches_preserve_each_record_and_atomic_outbox() {
    let db = fixture::TestDb::new().await;
    let first = (0..65).map(|_| record()).collect::<Vec<_>>();
    let second = (0..65).map(|_| record()).collect::<Vec<_>>();
    let (a, b) = tokio::join!(
        db.a.record_gateway_audit_events(GatewayAuditKind::Policy, &first),
        db.b.record_gateway_audit_events(GatewayAuditKind::Policy, &second),
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(
        db.a.gateway_audit_event_count(GatewayAuditKind::Policy)
            .await
            .unwrap(),
        130
    );
    let outbox = db.b.read_outbox(0, 1000).await.unwrap();
    let events = outbox
        .events
        .iter()
        .filter(|e| e.event_type == "gateway.audit.recorded")
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 130);
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
    // A late duplicate must roll back the earlier new record in this batch.
    let pending = record();
    assert!(
        db.a.record_gateway_audit_events(
            GatewayAuditKind::Policy,
            &[pending.clone(), first[0].clone()]
        )
        .await
        .is_err()
    );
    let absent: Option<AuditEventRecord> = db.b.client().select(pending.id).await.unwrap();
    assert!(absent.is_none());
    assert_eq!(
        db.b.gateway_audit_event_count(GatewayAuditKind::Policy)
            .await
            .unwrap(),
        130
    );
    let after = db.b.read_outbox(0, 1000).await.unwrap();
    assert_eq!(after.next_sequence, outbox.next_sequence);
}
