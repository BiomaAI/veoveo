//! Native SQL admission, idempotency, transitions and rollback in an owned Store fixture.
use chrono::{TimeDelta, Utc};
use std::{collections::BTreeMap, time::Duration};
use veoveo_platform_store::*;
use veoveo_recording_store::RecordingRepository;
use veoveo_recording_store::RecordingStoreError;
use veoveo_recording_store::{
    RecordingAccessScope, RecordingDatasetDraft, RecordingDatasetId, RecordingDraft, RecordingId,
    RecordingProjectionReceiptDraft, RecordingProjectionReceiptId,
    RecordingProjectionReceiptRecord, RecordingProjectionRequest, RecordingProjectionState,
    RecordingReadGrantClass, RecordingReadGrantDraft, RecordingReadGrantId,
    RecordingReadGrantRequest,
};
use veoveo_types::{DataLabelId, PolicyVersion, Sha256Digest};
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;

struct ProjectionFixture {
    scope: RecordingAccessScope,
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
        let scope = RecordingAccessScope {
            tenant_id: identity.tenant_id,
            actor_id: identity.principal_id,
            work_context_id: deterministic_work_context_id(&identity.tenant_key, "operations")
                .unwrap(),
            policy_revision: PolicyVersion::parse("r1").unwrap(),
            data_labels: [DataLabelId::parse("operations").unwrap()]
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
        let dataset = RecordingRepository::new(store.clone())
            .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
                identity.clone(),
                "projections",
            ))
            .await
            .unwrap();
        let dataset_id = RecordingDatasetId::from_uuid(uuid_key(&dataset.id));
        let recording = RecordingRepository::new(store.clone())
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
        let grant = RecordingRepository::new(store.clone())
            .create_recording_read_grant(RecordingReadGrantDraft {
                scope: scope.clone(),
                request: RecordingReadGrantRequest::new(
                    dataset_id,
                    RecordingReadGrantClass::AppProjection,
                    vec![recording_id],
                    "catalog-1",
                )
                .unwrap(),
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
fn assert_transition_denied(result: Result<RecordingProjectionReceiptRecord, RecordingStoreError>) {
    assert!(
        matches!(
            result,
            Err(RecordingStoreError::RecordingProjectionConflict { .. })
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn projection_authority_and_transitions_are_atomic() {
    let db = fixture::TestDb::with_modules(vec![
        veoveo_recording_store::schema::module_setup(
            fixture::module_lanes::execution("recordings").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    tokio::time::timeout(Duration::from_secs(90), qualify(&db))
        .await
        .expect("projection SQL qualification exceeded 90 seconds");
}
#[tokio::test]
async fn projection_transactions_use_the_rocksdb_profile() {
    let db = fixture::TestDb::with_backend_and_modules(
        fixture::StoreBackend::RocksDb,
        vec![
            veoveo_recording_store::schema::module_setup(
                fixture::module_lanes::execution("recordings").unwrap(),
            )
            .unwrap(),
        ],
    )
    .await;
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
        RecordingRepository::new(db.a.clone())
            .recording_projection_by_idempotency_key(&f.scope, &f.request("same"))
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
            RecordingRepository::new(db.a.clone())
                .reserve_recording_projection(draft)
                .await,
            Err(RecordingStoreError::RecordingReadGrantConflict { .. })
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
            RecordingRepository::new(db.a.clone())
                .reserve_recording_projection(draft)
                .await,
            Err(RecordingStoreError::RecordingReadGrantConflict { .. })
        ));
    }
    db.a.client()
        .query(include_str!("queries/recording_projections/qualify.surql"))
        .bind(("grant", f.grant.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        RecordingRepository::new(db.a.clone())
            .reserve_recording_projection(f.draft("wrong-class"))
            .await,
        Err(RecordingStoreError::RecordingReadGrantConflict { .. })
    ));
    db.a.client()
        .query(include_str!(
            "queries/recording_projections/qualify_2.surql"
        ))
        .bind(("grant", f.grant.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let recording_repository_a = RecordingRepository::new(db.a.clone());
    let recording_repository_b = RecordingRepository::new(db.b.clone());
    let (a, b) = tokio::join!(
        recording_repository_a.reserve_recording_projection(f.draft("same")),
        recording_repository_b.reserve_recording_projection(f.draft("same"))
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.id, b.id);
    let id = receipt_id(&a);
    let reused = RecordingRepository::new(db.b.clone())
        .recording_projection_by_idempotency_key(&f.scope, &f.request("same"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a, reused);
    assert_transition_denied(
        RecordingRepository::new(db.a.clone())
            .complete_recording_projection(&f.scope, f.recording, id, 4, &digest('c'))
            .await,
    );
    assert_transition_denied(
        RecordingRepository::new(db.a.clone())
            .begin_recording_projection(&f.scope, RecordingId::new(), id)
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
    s.policy_revision = PolicyVersion::parse("r2").unwrap();
    denied_scopes.push(s);
    let mut s = f.scope.clone();
    s.data_labels.clear();
    denied_scopes.push(s);
    for scope in &denied_scopes {
        let mut draft = f.draft("denied-new");
        draft.scope = scope.clone();
        assert!(matches!(
            RecordingRepository::new(db.a.clone())
                .reserve_recording_projection(draft)
                .await,
            Err(RecordingStoreError::RecordingReadGrantConflict { .. })
        ));
        assert_transition_denied(
            RecordingRepository::new(db.b.clone())
                .begin_recording_projection(scope, f.recording, id)
                .await,
        );
        assert_transition_denied(
            RecordingRepository::new(db.b.clone())
                .complete_recording_projection(scope, f.recording, id, 4, &digest('c'))
                .await,
        );
        assert_transition_denied(
            RecordingRepository::new(db.b.clone())
                .fail_recording_projection(scope, f.recording, id, "failure")
                .await,
        );
        assert_transition_denied(
            RecordingRepository::new(db.b.clone())
                .cancel_recording_projection(scope, f.recording, id, "cancelled")
                .await,
        );
    }
    for scope in &denied_scopes[2..] {
        assert!(matches!(
            RecordingRepository::new(db.a.clone())
                .recording_projection_by_idempotency_key(scope, &f.request("same"))
                .await,
            Err(RecordingStoreError::RecordingProjectionRequestConflict)
        ));
        let mut draft = f.draft("same");
        draft.scope = scope.clone();
        assert!(matches!(
            RecordingRepository::new(db.a.clone())
                .reserve_recording_projection(draft)
                .await,
            Err(RecordingStoreError::RecordingProjectionRequestConflict)
        ));
    }
    let wrong_digest =
        RecordingProjectionRequest::new(f.dataset, f.recording, "same", digest('c'), digest('b'))
            .unwrap();
    assert!(matches!(
        RecordingRepository::new(db.a.clone())
            .recording_projection_by_idempotency_key(&f.scope, &wrong_digest)
            .await,
        Err(RecordingStoreError::RecordingProjectionRequestConflict)
    ));
    let mut mismatch = f.draft("same");
    mismatch.request = wrong_digest;
    assert!(matches!(
        RecordingRepository::new(db.a.clone())
            .reserve_recording_projection(mismatch)
            .await,
        Err(RecordingStoreError::RecordingProjectionRequestConflict)
    ));
    let first = RecordingRepository::new(db.a.clone())
        .begin_recording_projection(&f.scope, f.recording, id)
        .await
        .unwrap();
    assert_eq!(first.state, RecordingProjectionState::Materializing);
    assert_eq!(
        first,
        RecordingRepository::new(db.b.clone())
            .begin_recording_projection(&f.scope, f.recording, id)
            .await
            .unwrap()
    );
    let ready = RecordingRepository::new(db.a.clone())
        .complete_recording_projection(&f.scope, f.recording, id, 4, &digest('c'))
        .await
        .unwrap();
    assert_eq!(ready.state, RecordingProjectionState::Ready);
    assert_eq!(
        ready,
        RecordingRepository::new(db.b.clone())
            .complete_recording_projection(&f.scope, f.recording, id, 4, &digest('c'))
            .await
            .unwrap()
    );
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .complete_recording_projection(&f.scope, f.recording, id, 5, &digest('c'))
            .await,
    );
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .complete_recording_projection(&f.scope, f.recording, id, 4, &digest('d'))
            .await,
    );
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .begin_recording_projection(&f.scope, f.recording, id)
            .await,
    );
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .cancel_recording_projection(&f.scope, f.recording, id, "cancelled")
            .await,
    );
    assert_eq!(
        ready,
        RecordingRepository::new(db.b.clone())
            .ready_recording_projection(&f.scope, f.recording, id)
            .await
            .unwrap()
            .unwrap()
    );

    for key in ["race-a", "race-b", "race-c", "race-d"] {
        let row = RecordingRepository::new(db.a.clone())
            .reserve_recording_projection(f.draft(key))
            .await
            .unwrap();
        let id = receipt_id(&row);
        RecordingRepository::new(db.a.clone())
            .begin_recording_projection(&f.scope, f.recording, id)
            .await
            .unwrap();
        let hash = digest('d');
        let recording_repository_a = RecordingRepository::new(db.a.clone());
        let recording_repository_b = RecordingRepository::new(db.b.clone());
        let (complete, cancel) = tokio::join!(
            recording_repository_a.complete_recording_projection(
                &f.scope,
                f.recording,
                id,
                8,
                &hash
            ),
            recording_repository_b.cancel_recording_projection(
                &f.scope,
                f.recording,
                id,
                "cancelled"
            )
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
    let cancelled = RecordingRepository::new(db.a.clone())
        .reserve_recording_projection(f.draft("cancelled"))
        .await
        .unwrap();
    let cancelled_id = receipt_id(&cancelled);
    let cancelled = RecordingRepository::new(db.a.clone())
        .cancel_recording_projection(&f.scope, f.recording, cancelled_id, "cancelled")
        .await
        .unwrap();
    assert_eq!(
        cancelled,
        RecordingRepository::new(db.b.clone())
            .cancel_recording_projection(&f.scope, f.recording, cancelled_id, "cancelled")
            .await
            .unwrap()
    );
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .cancel_recording_projection(&f.scope, f.recording, cancelled_id, "different")
            .await,
    );
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .fail_recording_projection(&f.scope, f.recording, cancelled_id, "failed")
            .await,
    );
    let failed = RecordingRepository::new(db.a.clone())
        .reserve_recording_projection(f.draft("failed"))
        .await
        .unwrap();
    let failed_id = receipt_id(&failed);
    let failed = RecordingRepository::new(db.a.clone())
        .fail_recording_projection(&f.scope, f.recording, failed_id, "failed")
        .await
        .unwrap();
    assert_eq!(
        failed,
        RecordingRepository::new(db.b.clone())
            .fail_recording_projection(&f.scope, f.recording, failed_id, "failed")
            .await
            .unwrap()
    );
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .begin_recording_projection(&f.scope, f.recording, failed_id)
            .await,
    );

    let row = RecordingRepository::new(db.a.clone())
        .reserve_recording_projection(f.draft("corrupt"))
        .await
        .unwrap();
    let corrupt = receipt_id(&row);
    db.a.client()
        .query(include_str!(
            "queries/recording_projections/qualify_3.surql"
        ))
        .bind(("row", corrupt.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    for scope in &denied_scopes {
        assert_transition_denied(
            RecordingRepository::new(db.b.clone())
                .begin_recording_projection(scope, f.recording, corrupt)
                .await,
        );
    }
    assert!(matches!(
        RecordingRepository::new(db.b.clone())
            .recording_projection_by_idempotency_key(&f.scope, &f.request("corrupt"))
            .await,
        Err(RecordingStoreError::RecordingProjectionRequestConflict)
    ));
    // A failure raised by an event must roll back the transition itself.
    let rollback = RecordingRepository::new(db.a.clone())
        .reserve_recording_projection(f.draft("rollback"))
        .await
        .unwrap();
    let rollback_id = receipt_id(&rollback);
    db.a.client()
        .query(include_str!(
            "queries/recording_projections/qualify_4.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        RecordingRepository::new(db.b.clone())
            .begin_recording_projection(&f.scope, f.recording, rollback_id)
            .await,
        Err(RecordingStoreError::Database(_))
    ));
    db.a.client()
        .query(include_str!(
            "queries/recording_projections/qualify_5.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        rollback,
        RecordingRepository::new(db.b.clone())
            .recording_projection_by_idempotency_key(&f.scope, &f.request("rollback"))
            .await
            .unwrap()
            .unwrap()
    );
    RecordingRepository::new(db.b.clone())
        .begin_recording_projection(&f.scope, f.recording, rollback_id)
        .await
        .unwrap();
    // Revocation is visible on the next transition and cannot be bypassed by a cached receipt.
    db.a.client()
        .query(include_str!(
            "queries/recording_projections/qualify_6.surql"
        ))
        .bind(("recording", f.recording.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .complete_recording_projection(&f.scope, f.recording, rollback_id, 4, &digest('c'))
            .await,
    );
    db.a.client()
        .query(include_str!(
            "queries/recording_projections/qualify_7.surql"
        ))
        .bind(("recording", f.recording.record_id()))
        .bind(("grant", f.grant.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_transition_denied(
        RecordingRepository::new(db.b.clone())
            .complete_recording_projection(&f.scope, f.recording, rollback_id, 4, &digest('c'))
            .await,
    );
    assert!(matches!(
        RecordingRepository::new(db.a.clone())
            .reserve_recording_projection(f.draft("expired-grant"))
            .await,
        Err(RecordingStoreError::RecordingReadGrantConflict { .. })
    ));
}
