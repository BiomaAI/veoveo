use super::*;
use crate::{
    contract::{ConvertFrameRequest, CoordinatePoint, CoordinateSpace, Wgs84Position},
    engine,
    test_store::TestDb,
};
use std::time::Duration;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime};

fn scope(
    tenant: Option<&str>,
    principal: &str,
    profile: &str,
    labels: &[&str],
) -> FrameOperationScope {
    FrameOperationScope::new(
        PrincipalId::new(principal).unwrap(),
        tenant.map(|t| TenantId::new(t).unwrap()),
        GatewayProfileId::new(profile).unwrap(),
        labels
            .iter()
            .map(|s| DataLabelId::new(*s).unwrap())
            .collect(),
    )
}

fn provenance() -> CoordinateOperationProvenance {
    engine::convert_frame(
        ConvertFrameRequest {
            target: CoordinateSpace::EcefWgs84,
            points: vec![CoordinatePoint::Wgs84(Wgs84Position {
                longitude_degrees: 0.,
                latitude_degrees: 0.,
                ellipsoid_height_m: 0.,
            })],
            allow_approximation: false,
        },
        &engine::ResolvedWorlds::default(),
    )
    .unwrap()
    .provenance
}

async fn task(db: &TestDb, s: &FrameOperationScope, server: &str) -> TaskId {
    let owner: TaskOwner = serde_json::from_value(serde_json::json!({
        "principal_key":s.principal, "principal_kind":"service", "issuer":"https://operation.test",
        "subject":s.principal, "profile":s.profile, "tenant_key":s.tenant,"data_labels":s.clearance,
        "authority":{"work_context":"operation-fixture","tenant":s.tenant_key(),
            "membership":"contributor","policy_revision":"test-1",
            "output_policy":{"owner":{"kind":"principal","id":s.principal}},
            "provenance":{"mode":"automated"}}
    }))
    .unwrap();
    let runtime = TaskRuntime::new(db.a.clone(), server, "writer");
    let id = TaskId::new();
    runtime
        .create(CreateTask {
            task_id: id,
            owner,
            server: server.to_owned(),
            task_type: const { veoveo_types::TaskTypeName::from_static("batch_transform") },
            request: serde_json::json!({}),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    id
}

async fn events(db: &TestDb, id: &CoordinateOperationId) -> usize {
    let mut response = db.b.client().query("SELECT VALUE id FROM outbox_event WHERE aggregate_type = 'coordinate_operation' AND aggregate_id = $key;")
        .bind(("key", id.to_string())).await.unwrap().check().unwrap();
    response.take::<Vec<RecordId>>(0).unwrap().len()
}

#[tokio::test]
async fn direct_operations_enforce_sql_authority_and_immutable_concurrent_replay() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = TestDb::new().await;
        let a = FramesState::new(db.a.clone());
        let b = FramesState::new(db.b.clone());
        let caller = scope(Some("tenant-a"), "owner", "operator", &["cui", "mission"]);
        let mut oversized = caller.clone();
        oversized
            .clearance
            .insert(DataLabelId::new("x".repeat(257)).unwrap());
        assert!(
            a.record_operation(&oversized, None, &provenance())
                .await
                .is_err()
        );
        let p = provenance();
        let uri = p.operation.operation_uri();
        let (first, replay) = tokio::join!(
            a.record_operation(&caller, None, &p),
            b.record_operation(&caller, None, &p)
        );
        first.unwrap();
        replay.unwrap();
        assert_eq!(
            b.get_operation(&caller, uri).await.unwrap(),
            Some(p.clone())
        );
        assert_eq!(events(&db, p.operation.operation_id()).await, 1);
        for denied in [
            scope(Some("tenant-a"), "other", "operator", &["cui", "mission"]),
            scope(Some("tenant-b"), "owner", "operator", &["cui", "mission"]),
            scope(Some("tenant-a"), "owner", "observer", &["cui", "mission"]),
            scope(Some("tenant-a"), "owner", "operator", &["cui"]),
        ] {
            assert!(b.get_operation(&denied, uri).await.unwrap().is_none());
            assert!(b.record_operation(&denied, None, &p).await.is_err());
        }
        let mut changed = p.clone();
        changed.engine = Some("different-engine".into());
        assert!(a.record_operation(&caller, None, &changed).await.is_err());
        assert_eq!(events(&db, p.operation.operation_id()).await, 1);
        db.a.client()
            .query("UPDATE ONLY $operation SET labels += 'secret' RETURN NONE;")
            .bind(("operation", record_id(p.operation.operation_id()).unwrap()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(b.get_operation(&caller, uri).await.unwrap().is_none());
        let mut cleared = caller.clone();
        cleared
            .clearance
            .insert(DataLabelId::new("secret").unwrap());
        assert!(b.get_operation(&cleared, uri).await.unwrap().is_some());
        // SQL must discard a denied record without trying to decode its provenance.
        db.a.client()
            .query("UPDATE ONLY $operation SET provenance = {} RETURN NONE;")
            .bind(("operation", record_id(p.operation.operation_id()).unwrap()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(b.get_operation(&caller, uri).await.unwrap().is_none());
        assert!(b.get_operation(&cleared, uri).await.is_err());
        let p = provenance();
        let none = scope(None, "owner", "operator", &[]);
        a.record_operation(&none, None, &p).await.unwrap();
        assert!(
            b.get_operation(
                &scope(Some("installation"), "owner", "operator", &[]),
                p.operation.operation_uri()
            )
            .await
            .unwrap()
            .is_none()
        );
    })
    .await
    .expect("operation replay qualification exceeded 60 seconds");
}

#[tokio::test]
async fn task_operations_check_current_parent_in_read_and_write_transactions() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = TestDb::new().await;
        let a = FramesState::new(db.a.clone());
        let b = FramesState::new(db.b.clone());
        let caller = scope(Some("tenant-a"), "owner", "operator", &["cui"]);
        for denied in [
            scope(Some("tenant-a"), "other", "operator", &["cui"]),
            scope(Some("tenant-b"), "owner", "operator", &["cui"]),
            scope(Some("tenant-a"), "owner", "observer", &["cui"]),
            scope(Some("tenant-a"), "owner", "operator", &["cui", "secret"]),
        ] {
            let id = task(&db, &denied, "frames").await;
            let p = provenance();
            assert!(a.record_operation(&caller, Some(id), &p).await.is_err());
            assert_eq!(events(&db, p.operation.operation_id()).await, 0);
        }
        let wrong_server = task(&db, &caller, "other").await;
        assert!(
            a.record_operation(&caller, Some(wrong_server), &provenance())
                .await
                .is_err()
        );
        for query in [
            "UPDATE ONLY $task SET owner = principal:other RETURN NONE;",
            "UPDATE ONLY $task SET tenant = tenant:other RETURN NONE;",
            "UPDATE ONLY $task SET profile = profile:other RETURN NONE;",
            "UPDATE ONLY $task SET server = mcp_server:other RETURN NONE;",
            "UPDATE ONLY $task SET request.owner.principal_key = 'other' RETURN NONE;",
            "UPDATE ONLY $task SET request.owner.profile = 'other' RETURN NONE;",
            "UPDATE ONLY $task SET request.owner.tenant_key = NONE RETURN NONE;",
            "UPDATE ONLY $task SET request.owner.data_labels += 'secret' RETURN NONE;",
            "DELETE $task RETURN NONE;",
        ] {
            let id = task(&db, &caller, "frames").await;
            let p = provenance();
            a.record_operation(&caller, Some(id), &p).await.unwrap();
            assert!(
                b.get_operation(&caller, p.operation.operation_uri())
                    .await
                    .unwrap()
                    .is_some()
            );
            db.a.client()
                .query(query)
                .bind(("task", task_record_id(id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                b.get_operation(&caller, p.operation.operation_uri())
                    .await
                    .unwrap()
                    .is_none(),
                "{query}"
            );
            assert!(
                a.record_operation(&caller, Some(id), &p).await.is_err(),
                "{query}"
            );
            let rejected = provenance();
            assert!(
                a.record_operation(&caller, Some(id), &rejected)
                    .await
                    .is_err(),
                "{query}"
            );
            assert_eq!(events(&db, rejected.operation.operation_id()).await, 0);
        }
        let none = scope(None, "owner", "operator", &[]);
        let id = task(&db, &none, "frames").await;
        assert!(
            a.record_operation(
                &scope(Some("installation"), "owner", "operator", &[]),
                Some(id),
                &provenance()
            )
            .await
            .is_err()
        );
    })
    .await
    .expect("operation Task qualification exceeded 60 seconds");
}

#[tokio::test]
async fn operation_schema_requires_profile_authority() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = TestDb::new().await;
        let state = FramesState::new(db.a.clone());
        let caller = scope(Some("tenant-a"), "owner", "operator", &[]);
        let provenance = provenance();
        state
            .record_operation(&caller, None, &provenance)
            .await
            .unwrap();
        let operation = record_id(provenance.operation.operation_id()).unwrap();
        for query in [
            "UPDATE ONLY $operation UNSET authority;",
            "UPDATE ONLY $operation UNSET authority.profile;",
        ] {
            assert!(
                db.b.client()
                    .query(query)
                    .bind(("operation", operation.clone()))
                    .await
                    .unwrap()
                    .check()
                    .is_err()
            );
            assert_eq!(
                state
                    .get_operation(&caller, provenance.operation.operation_uri())
                    .await
                    .unwrap(),
                Some(provenance.clone())
            );
        }
        state
            .record_operation(&caller, None, &provenance)
            .await
            .unwrap();
        assert_eq!(events(&db, provenance.operation.operation_id()).await, 1);
    })
    .await
    .expect("operation authority schema qualification exceeded 60 seconds");
}

#[test]
fn persisted_operation_keys_require_uuid_v7() {
    assert!(
        record_id(&CoordinateOperationId::new(format!("op-{}", uuid::Uuid::now_v7())).unwrap())
            .is_some()
    );
    for key in [
        "op-test",
        "op-550e8400-e29b-41d4-a716-446655440000",
        "01950000-0000-7000-8000-000000000001",
    ] {
        assert!(record_id(&CoordinateOperationId::new(key).unwrap()).is_none());
    }
}

#[tokio::test]
async fn outbox_failure_rolls_back_operation_and_allows_retry() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db=TestDb::new().await; let state=FramesState::new(db.a.clone());
        let caller=scope(Some("tenant-a"),"owner","operator", &[]);let p=provenance();
        db.a.client().query("DEFINE EVENT reject_operation_event ON TABLE outbox_event WHEN $after.aggregate_type = 'coordinate_operation' THEN { THROW 'fixture rejected outbox publication'; };")
            .await.unwrap().check().unwrap();
        assert!(state.record_operation(&caller,None,&p).await.is_err());
        assert!(state.get_operation(&caller,p.operation.operation_uri()).await.unwrap().is_none());
        assert_eq!(events(&db,p.operation.operation_id()).await,0);
        db.a.client().query("REMOVE EVENT reject_operation_event ON TABLE outbox_event;").await.unwrap().check().unwrap();
        state.record_operation(&caller,None,&p).await.unwrap();
        assert!(state.get_operation(&caller,p.operation.operation_uri()).await.unwrap().is_some());
        assert_eq!(events(&db,p.operation.operation_id()).await,1);
    }).await.expect("operation rollback qualification exceeded 60 seconds");
}
