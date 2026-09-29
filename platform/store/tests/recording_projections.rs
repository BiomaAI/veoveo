//! Native SQL admission, idempotency, transitions and rollback in an owned Store fixture.
use chrono::{TimeDelta, Utc};
use std::{collections::BTreeMap, time::Duration};
use veoveo_platform_store::*;
use veoveo_types::{DataLabelId, PolicyVersion, Sha256Digest};
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

struct ProjectionFixture {
    scope: RecordingProjectionScope,
    dataset: RecordingDatasetId,
    recording: RecordingId,
    grant: RecordingReadGrantId,
}
fn digest(hex: char) -> Sha256Digest {
    Sha256Digest::from_hex(hex.to_string().repeat(64)).unwrap()
}
impl ProjectionFixture {
    async fn create(store: &PlatformStore) -> Self {
        let identity = store
            .ensure_identity(
                "projections",
                "author",
                "https://fixture.invalid",
                "author",
                PrincipalKind::User,
            )
            .await
            .unwrap();
        let scope = RecordingProjectionScope {
            tenant_id: identity.tenant_id,
            actor_id: identity.principal_id,
            work_context_id: deterministic_work_context_id(&identity.tenant_key, "operations")
                .unwrap(),
            policy_revision: PolicyVersion::new("r1").unwrap(),
            data_labels: [DataLabelId::new("operations").unwrap()]
                .into_iter()
                .collect(),
        };
        let authority = InvocationAuthorityRecord {
            context_key: "operations".into(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: "r1".into(),
            owner_kind: ArtifactGrantSubjectKind::Principal,
            owner_key: identity.principal_key.clone(),
            initial_grants: vec![],
            classification: None,
            data_labels: vec!["operations".into()],
            invocation_mode: InvocationMode::Direct,
            initiator_key: Some(identity.principal_key.clone()),
            delegation_id: None,
        };
        let dataset = store
            .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
                identity.clone(),
                "projections",
            ))
            .await
            .unwrap();
        let dataset_id = RecordingDatasetId::from_uuid(uuid_key(&dataset.id));
        let recording = store
            .create_recording(RecordingDraft {
                identity: identity.clone(),
                authority: authority.clone(),
                dataset_id,
                application_id: dataset_id.to_string(),
                recording_key: "source".into(),
                classification: "unclassified".into(),
                labels: vec!["operations".into()],
                metadata: BTreeMap::new(),
                started_at: Utc::now(),
            })
            .await
            .unwrap();
        let recording_id = RecordingId::from_uuid(uuid_key(&recording.id));
        let grant = store
            .create_recording_read_grant(RecordingReadGrantDraft {
                identity,
                authority,
                dataset_id,
                grant_class: RecordingReadGrantClass::AppProjection,
                recording_ids: vec![recording_id],
                catalog_revision: "catalog-1".into(),
                expires_at: Utc::now() + TimeDelta::minutes(10),
            })
            .await
            .unwrap();
        Self {
            scope,
            dataset: dataset_id,
            recording: recording_id,
            grant: RecordingReadGrantId::from_uuid(uuid_key(&grant.id)),
        }
    }
    fn request(&self, key: &str) -> RecordingProjectionRequest {
        RecordingProjectionRequest::new(self.dataset, self.recording, key, digest('a'), digest('b'))
            .unwrap()
    }
    fn draft(&self, key: &str) -> RecordingProjectionReceiptDraft {
        RecordingProjectionReceiptDraft {
            scope: self.scope.clone(),
            request: self.request(key),
            grant_id: self.grant,
            expires_at: Utc::now() + TimeDelta::minutes(5),
        }
    }
}
fn uuid_key(id: &RecordId) -> uuid::Uuid {
    let RecordIdKey::Uuid(id) = &id.key else {
        panic!("expected native UUID key")
    };
    id.into_inner()
}
fn receipt_id(row: &RecordingProjectionReceiptRecord) -> RecordingProjectionReceiptId {
    RecordingProjectionReceiptId::from_uuid(uuid_key(&row.id))
}
fn assert_transition_denied(result: Result<RecordingProjectionReceiptRecord, StoreError>) {
    assert!(
        matches!(result, Err(StoreError::RecordingProjectionConflict { .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn projection_authority_and_transitions_are_atomic() {
    let db = fixture::TestDb::new().await;
    tokio::time::timeout(Duration::from_secs(90), qualify(&db))
        .await
        .expect("projection SQL qualification exceeded 90 seconds");
}
#[tokio::test]
async fn projection_transactions_use_the_rocksdb_profile() {
    let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
    tokio::time::timeout(Duration::from_secs(90), qualify(&db))
        .await
        .expect("projection RocksDB qualification exceeded 90 seconds");
}
async fn qualify(db: &fixture::TestDb) {
    let f = ProjectionFixture::create(&db.a).await;
    assert!(
        RecordingProjectionRequest::new(f.dataset, f.recording, "", digest('a'), digest('b'))
            .is_err()
    );
    assert!(
        RecordingProjectionRequest::new(
            f.dataset,
            f.recording,
            "x".repeat(129),
            digest('a'),
            digest('b')
        )
        .is_err()
    );
    assert!(
        db.a.recording_projection_by_idempotency_key(&f.scope, &f.request("same"))
            .await
            .unwrap()
            .is_none()
    );
    for expires_at in [
        Utc::now() - TimeDelta::seconds(1),
        Utc::now() + TimeDelta::hours(1),
    ] {
        let mut draft = f.draft("invalid-expiry");
        draft.expires_at = expires_at;
        assert!(matches!(
            db.a.reserve_recording_projection(draft).await,
            Err(StoreError::RecordingReadGrantConflict { .. })
        ));
    }
    for (dataset, recording) in [
        (RecordingDatasetId::new(), f.recording),
        (f.dataset, RecordingId::new()),
    ] {
        let mut draft = f.draft("wrong-parent");
        draft.request = RecordingProjectionRequest::new(
            dataset,
            recording,
            "wrong-parent",
            digest('a'),
            digest('b'),
        )
        .unwrap();
        assert!(matches!(
            db.a.reserve_recording_projection(draft).await,
            Err(StoreError::RecordingReadGrantConflict { .. })
        ));
    }
    db.a.client()
        .query("UPDATE $grant SET grant_class = 'viewer_segment' RETURN NONE;")
        .bind(("grant", f.grant.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        db.a.reserve_recording_projection(f.draft("wrong-class"))
            .await,
        Err(StoreError::RecordingReadGrantConflict { .. })
    ));
    db.a.client()
        .query("UPDATE $grant SET grant_class = 'app_projection' RETURN NONE;")
        .bind(("grant", f.grant.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let (a, b) = tokio::join!(
        db.a.reserve_recording_projection(f.draft("same")),
        db.b.reserve_recording_projection(f.draft("same"))
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.id, b.id);
    let id = receipt_id(&a);
    let reused =
        db.b.recording_projection_by_idempotency_key(&f.scope, &f.request("same"))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(a, reused);
    assert_transition_denied(
        db.a.complete_recording_projection(&f.scope, f.recording, id, 4, &digest('c'))
            .await,
    );
    assert_transition_denied(
        db.a.begin_recording_projection(&f.scope, RecordingId::new(), id)
            .await,
    );
    let mut denied_scopes = Vec::new();
    let mut s = f.scope.clone();
    s.actor_id = PrincipalId::new();
    denied_scopes.push(s);
    let mut s = f.scope.clone();
    s.tenant_id = TenantId::new();
    denied_scopes.push(s);
    let mut s = f.scope.clone();
    s.work_context_id = WorkContextId::new();
    denied_scopes.push(s);
    let mut s = f.scope.clone();
    s.policy_revision = PolicyVersion::new("r2").unwrap();
    denied_scopes.push(s);
    let mut s = f.scope.clone();
    s.data_labels.clear();
    denied_scopes.push(s);
    for scope in &denied_scopes {
        let mut draft = f.draft("denied-new");
        draft.scope = scope.clone();
        assert!(matches!(
            db.a.reserve_recording_projection(draft).await,
            Err(StoreError::RecordingReadGrantConflict { .. })
        ));
        assert_transition_denied(
            db.b.begin_recording_projection(scope, f.recording, id)
                .await,
        );
        assert_transition_denied(
            db.b.complete_recording_projection(scope, f.recording, id, 4, &digest('c'))
                .await,
        );
        assert_transition_denied(
            db.b.fail_recording_projection(scope, f.recording, id, "failure")
                .await,
        );
        assert_transition_denied(
            db.b.cancel_recording_projection(scope, f.recording, id, "cancelled")
                .await,
        );
    }
    for scope in &denied_scopes[2..] {
        assert!(matches!(
            db.a.recording_projection_by_idempotency_key(scope, &f.request("same"))
                .await,
            Err(StoreError::RecordingProjectionRequestConflict)
        ));
        let mut draft = f.draft("same");
        draft.scope = scope.clone();
        assert!(matches!(
            db.a.reserve_recording_projection(draft).await,
            Err(StoreError::RecordingProjectionRequestConflict)
        ));
    }
    let wrong_digest =
        RecordingProjectionRequest::new(f.dataset, f.recording, "same", digest('c'), digest('b'))
            .unwrap();
    assert!(matches!(
        db.a.recording_projection_by_idempotency_key(&f.scope, &wrong_digest)
            .await,
        Err(StoreError::RecordingProjectionRequestConflict)
    ));
    let mut mismatch = f.draft("same");
    mismatch.request = wrong_digest;
    assert!(matches!(
        db.a.reserve_recording_projection(mismatch).await,
        Err(StoreError::RecordingProjectionRequestConflict)
    ));
    let first =
        db.a.begin_recording_projection(&f.scope, f.recording, id)
            .await
            .unwrap();
    assert_eq!(first.state, RecordingProjectionState::Materializing);
    assert_eq!(
        first,
        db.b.begin_recording_projection(&f.scope, f.recording, id)
            .await
            .unwrap()
    );
    let ready =
        db.a.complete_recording_projection(&f.scope, f.recording, id, 4, &digest('c'))
            .await
            .unwrap();
    assert_eq!(ready.state, RecordingProjectionState::Ready);
    assert_eq!(
        ready,
        db.b.complete_recording_projection(&f.scope, f.recording, id, 4, &digest('c'))
            .await
            .unwrap()
    );
    assert_transition_denied(
        db.b.complete_recording_projection(&f.scope, f.recording, id, 5, &digest('c'))
            .await,
    );
    assert_transition_denied(
        db.b.complete_recording_projection(&f.scope, f.recording, id, 4, &digest('d'))
            .await,
    );
    assert_transition_denied(
        db.b.begin_recording_projection(&f.scope, f.recording, id)
            .await,
    );
    assert_transition_denied(
        db.b.cancel_recording_projection(&f.scope, f.recording, id, "cancelled")
            .await,
    );
    assert_eq!(
        ready,
        db.b.ready_recording_projection(&f.scope, f.recording, id)
            .await
            .unwrap()
            .unwrap()
    );

    for key in ["race-a", "race-b", "race-c", "race-d"] {
        let row =
            db.a.reserve_recording_projection(f.draft(key))
                .await
                .unwrap();
        let id = receipt_id(&row);
        db.a.begin_recording_projection(&f.scope, f.recording, id)
            .await
            .unwrap();
        let hash = digest('d');
        let (complete, cancel) = tokio::join!(
            db.a.complete_recording_projection(&f.scope, f.recording, id, 8, &hash),
            db.b.cancel_recording_projection(&f.scope, f.recording, id, "cancelled")
        );
        assert_eq!(
            usize::from(complete.is_ok()) + usize::from(cancel.is_ok()),
            1,
            "complete={complete:?}; cancel={cancel:?}"
        );
        if complete.is_ok() {
            assert_transition_denied(cancel);
        } else {
            assert_transition_denied(complete);
        }
    }
    let cancelled =
        db.a.reserve_recording_projection(f.draft("cancelled"))
            .await
            .unwrap();
    let cancelled_id = receipt_id(&cancelled);
    let cancelled =
        db.a.cancel_recording_projection(&f.scope, f.recording, cancelled_id, "cancelled")
            .await
            .unwrap();
    assert_eq!(
        cancelled,
        db.b.cancel_recording_projection(&f.scope, f.recording, cancelled_id, "cancelled")
            .await
            .unwrap()
    );
    assert_transition_denied(
        db.b.cancel_recording_projection(&f.scope, f.recording, cancelled_id, "different")
            .await,
    );
    assert_transition_denied(
        db.b.fail_recording_projection(&f.scope, f.recording, cancelled_id, "failed")
            .await,
    );
    let failed =
        db.a.reserve_recording_projection(f.draft("failed"))
            .await
            .unwrap();
    let failed_id = receipt_id(&failed);
    let failed =
        db.a.fail_recording_projection(&f.scope, f.recording, failed_id, "failed")
            .await
            .unwrap();
    assert_eq!(
        failed,
        db.b.fail_recording_projection(&f.scope, f.recording, failed_id, "failed")
            .await
            .unwrap()
    );
    assert_transition_denied(
        db.b.begin_recording_projection(&f.scope, f.recording, failed_id)
            .await,
    );

    let row =
        db.a.reserve_recording_projection(f.draft("corrupt"))
            .await
            .unwrap();
    let corrupt = receipt_id(&row);
    db.a.client().query("DEFINE FIELD OVERWRITE manifest_digest ON recording_projection_receipt TYPE any; UPDATE $row SET manifest_digest = 17 RETURN NONE;")
        .bind(("row",corrupt.record_id())).await.unwrap().check().unwrap();
    for scope in &denied_scopes {
        assert_transition_denied(
            db.b.begin_recording_projection(scope, f.recording, corrupt)
                .await,
        );
    }
    assert!(matches!(
        db.b.recording_projection_by_idempotency_key(&f.scope, &f.request("corrupt"))
            .await,
        Err(StoreError::RecordingProjectionRequestConflict)
    ));
    // A failure raised by an event must roll back the transition itself.
    let rollback =
        db.a.reserve_recording_projection(f.draft("rollback"))
            .await
            .unwrap();
    let rollback_id = receipt_id(&rollback);
    db.a.client().query("DEFINE EVENT projection_failure ON recording_projection_receipt WHEN $event = 'UPDATE' AND $after.state = 'materializing' THEN { THROW 'injected_projection_failure'; };").await.unwrap().check().unwrap();
    assert!(matches!(
        db.b.begin_recording_projection(&f.scope, f.recording, rollback_id)
            .await,
        Err(StoreError::Database(_))
    ));
    db.a.client()
        .query("REMOVE EVENT projection_failure ON recording_projection_receipt;")
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        rollback,
        db.b.recording_projection_by_idempotency_key(&f.scope, &f.request("rollback"))
            .await
            .unwrap()
            .unwrap()
    );
    db.b.begin_recording_projection(&f.scope, f.recording, rollback_id)
        .await
        .unwrap();
    // Revocation is visible on the next transition and cannot be bypassed by a cached receipt.
    db.a.client()
        .query("UPDATE $recording SET labels = ['restricted'] RETURN NONE;")
        .bind(("recording", f.recording.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_transition_denied(
        db.b.complete_recording_projection(&f.scope, f.recording, rollback_id, 4, &digest('c'))
            .await,
    );
    db.a.client().query("UPDATE $recording SET labels = ['operations'] RETURN NONE; UPDATE $grant SET expires_at = time::now() - 1s RETURN NONE;")
        .bind(("recording",f.recording.record_id())).bind(("grant",f.grant.record_id())).await.unwrap().check().unwrap();
    assert_transition_denied(
        db.b.complete_recording_projection(&f.scope, f.recording, rollback_id, 4, &digest('c'))
            .await,
    );
    assert!(matches!(
        db.a.reserve_recording_projection(f.draft("expired-grant"))
            .await,
        Err(StoreError::RecordingReadGrantConflict { .. })
    ));
}
