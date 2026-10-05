//! Grant admission runs against native SurrealDB, with owned fixtures and two clients.
use chrono::{TimeDelta, Utc};
use std::{collections::BTreeMap, time::Duration};
use veoveo_platform_store::*;
use veoveo_recording_store::RecordingRepository;
use veoveo_recording_store::RecordingStoreError;
use veoveo_recording_store::{
    RecordingAccessScope, RecordingDatasetDraft, RecordingDatasetId, RecordingDraft, RecordingId,
    RecordingReadGrantClass, RecordingReadGrantDraft, RecordingReadGrantId,
    RecordingReadGrantRecord, RecordingReadGrantRequest,
};
use veoveo_types::{DataLabelId, PolicyVersion};
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;

struct Grants {
    scope: RecordingAccessScope,
    dataset: RecordingDatasetId,
    recordings: Vec<RecordingId>,
}
impl Grants {
    async fn new(store: &PlatformStore) -> Self {
        let identity = store
            .ensure_identity(
                "grants",
                "reader",
                "https://fixture.invalid",
                "reader",
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
                "source",
            ))
            .await
            .unwrap();
        let dataset = RecordingDatasetId::from_uuid(uuid_key(&dataset.id));
        let mut recordings = Vec::new();
        for key in ["first", "second"] {
            let recording = RecordingRepository::new(store.clone())
                .create_recording(RecordingDraft {
                    identity: identity.clone(),
                    authority: authority.clone(),
                    dataset_id: dataset,
                    application_id: dataset.to_string(),
                    recording_key: key.into(),
                    classification: "unclassified".into(),
                    labels: vec!["operations".into()],
                    metadata: BTreeMap::new(),
                    started_at: Utc::now(),
                })
                .await
                .unwrap();
            recordings.push(RecordingId::from_uuid(uuid_key(&recording.id)));
        }
        Self {
            scope,
            dataset,
            recordings,
        }
    }
    fn request(&self, class: RecordingReadGrantClass) -> RecordingReadGrantRequest {
        let ids = if class == RecordingReadGrantClass::CatalogDataset {
            self.recordings.clone()
        } else {
            vec![self.recordings[0]]
        };
        RecordingReadGrantRequest::new(self.dataset, class, ids, "catalog-1").unwrap()
    }
    fn draft(&self, class: RecordingReadGrantClass) -> RecordingReadGrantDraft {
        RecordingReadGrantDraft {
            scope: self.scope.clone(),
            request: self.request(class),
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
async fn count(store: &PlatformStore) -> i64 {
    store
        .client()
        .query(include_str!("queries/recording_grants/count.surql"))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take::<Option<i64>>(0)
        .unwrap()
        .unwrap()
}
fn denied(result: Result<RecordingReadGrantRecord, RecordingStoreError>) {
    assert!(
        matches!(
            result,
            Err(RecordingStoreError::RecordingReadGrantConflict { .. })
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn grant_admission_and_redemption_use_sql() {
    let db = fixture::TestDb::with_modules(vec![
        veoveo_recording_store::schema::module_setup(
            fixture::module_lanes::execution("recordings").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    tokio::time::timeout(Duration::from_secs(90), qualify(&db))
        .await
        .expect("grant qualification exceeded 90 seconds");
}
#[tokio::test]
async fn grant_transactions_use_the_rocksdb_profile() {
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
        .expect("grant RocksDB qualification exceeded 90 seconds");
}
async fn qualify(db: &fixture::TestDb) {
    let f = Grants::new(&db.a).await;
    let class = RecordingReadGrantClass::CatalogDataset;
    let request = f.request(class);
    let grant = RecordingRepository::new(db.a.clone())
        .create_recording_read_grant(f.draft(class))
        .await
        .unwrap();
    let id = RecordingReadGrantId::from_uuid(uuid_key(&grant.id));
    assert_eq!(
        Some(grant.clone()),
        RecordingRepository::new(db.b.clone())
            .reusable_recording_read_grant(&f.scope, &request, id)
            .await
            .unwrap()
    );
    assert_eq!(
        Some(grant.clone()),
        RecordingRepository::new(db.b.clone())
            .recording_redap_grant(id)
            .await
            .unwrap()
    );
    assert_eq!(grant.work_context, f.scope.work_context_id.record_id());
    for (class, redap) in [
        (RecordingReadGrantClass::ViewerSegment, true),
        (RecordingReadGrantClass::AppProjection, false),
    ] {
        let row = RecordingRepository::new(db.a.clone())
            .create_recording_read_grant(f.draft(class))
            .await
            .unwrap();
        let id = RecordingReadGrantId::from_uuid(uuid_key(&row.id));
        assert_eq!(
            RecordingRepository::new(db.b.clone())
                .recording_redap_grant(id)
                .await
                .unwrap()
                .is_some(),
            redap
        );
    }
    assert!(
        RecordingRepository::new(db.b.clone())
            .recording_redap_grant(RecordingReadGrantId::new())
            .await
            .unwrap()
            .is_none()
    );

    let mut denied_scopes = Vec::new();
    let mut scope = f.scope.clone();
    scope.tenant_id = TenantId::new();
    denied_scopes.push(scope);
    let mut scope = f.scope.clone();
    scope.actor_id = PrincipalId::new();
    denied_scopes.push(scope);
    let mut scope = f.scope.clone();
    scope.work_context_id = WorkContextId::new();
    denied_scopes.push(scope);
    let mut scope = f.scope.clone();
    scope.policy_revision = PolicyVersion::parse("r2").unwrap();
    denied_scopes.push(scope);
    let mut scope = f.scope.clone();
    scope.data_labels.clear();
    denied_scopes.push(scope);
    // Corrupt an unrelated field: denied requests must never deserialize this row.
    db.a.client()
        .query(include_str!("queries/recording_grants/qualify.surql"))
        .bind(("grant", id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        RecordingRepository::new(db.b.clone())
            .reusable_recording_read_grant(&f.scope, &request, id)
            .await
            .is_err()
    );
    assert!(
        RecordingRepository::new(db.b.clone())
            .recording_redap_grant(id)
            .await
            .is_err()
    );
    for scope in &denied_scopes {
        assert!(
            RecordingRepository::new(db.b.clone())
                .reusable_recording_read_grant(scope, &request, id)
                .await
                .unwrap()
                .is_none()
        );
    }
    for request in [
        RecordingReadGrantRequest::new(
            RecordingDatasetId::new(),
            class,
            f.recordings.clone(),
            "catalog-1",
        )
        .unwrap(),
        RecordingReadGrantRequest::new(f.dataset, class, vec![f.recordings[0]], "catalog-1")
            .unwrap(),
        RecordingReadGrantRequest::new(f.dataset, class, f.recordings.clone(), "different")
            .unwrap(),
        f.request(RecordingReadGrantClass::ViewerSegment),
    ] {
        assert!(
            RecordingRepository::new(db.b.clone())
                .reusable_recording_read_grant(&f.scope, &request, id)
                .await
                .unwrap()
                .is_none()
        );
    }
    for (_field, statement) in [
        (
            "grant_class = 'app_projection'",
            include_str!("queries/recording_grants/qualify_2_variant_1.surql"),
        ),
        (
            "expires_at = time::now() - 1s",
            include_str!("queries/recording_grants/qualify_2_variant_2.surql"),
        ),
    ] {
        db.a.client()
            .query(statement)
            .bind(("grant", id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            RecordingRepository::new(db.b.clone())
                .recording_redap_grant(id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            RecordingRepository::new(db.b.clone())
                .reusable_recording_read_grant(&f.scope, &request, id)
                .await
                .unwrap()
                .is_none()
        );
        db.a.client()
            .query(include_str!("queries/recording_grants/qualify_3.surql"))
            .bind(("grant", id.record_id()))
            .bind(("class", class))
            .bind(("expires", grant.expires_at))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    db.a.client()
        .query(include_str!("queries/recording_grants/qualify_4.surql"))
        .bind(("grant", id.record_id()))
        .bind(("created", grant.created_at))
        .await
        .unwrap()
        .check()
        .unwrap();

    let baseline = count(&db.a).await;
    for scope in [denied_scopes[0].clone(), denied_scopes[4].clone()] {
        let mut draft = f.draft(class);
        draft.scope = scope;
        denied(
            RecordingRepository::new(db.b.clone())
                .create_recording_read_grant(draft)
                .await,
        );
    }
    for expiry in [
        Utc::now() - TimeDelta::seconds(1),
        Utc::now() + TimeDelta::hours(2),
    ] {
        let mut draft = f.draft(class);
        draft.expires_at = expiry;
        denied(
            RecordingRepository::new(db.b.clone())
                .create_recording_read_grant(draft)
                .await,
        );
    }
    for (dataset, recordings) in [
        (RecordingDatasetId::new(), f.recordings.clone()),
        (f.dataset, vec![f.recordings[0], RecordingId::new()]),
    ] {
        let mut draft = f.draft(class);
        draft.request =
            RecordingReadGrantRequest::new(dataset, class, recordings, "catalog-1").unwrap();
        denied(
            RecordingRepository::new(db.b.clone())
                .create_recording_read_grant(draft)
                .await,
        );
    }
    assert_eq!(baseline, count(&db.a).await);

    // The complete set is admitted; revoking just its second Recording denies both paths.
    for (_assignment, statement) in [
        (
            "labels = ['restricted']",
            include_str!("queries/recording_grants/qualify_5_variant_1.surql"),
        ),
        (
            "tenant = $other_tenant",
            include_str!("queries/recording_grants/qualify_5_variant_2.surql"),
        ),
        (
            "dataset = $other_dataset",
            include_str!("queries/recording_grants/qualify_5_variant_3.surql"),
        ),
    ] {
        db.a.client()
            .query(statement)
            .bind(("recording", f.recordings[1].record_id()))
            .bind(("other_tenant", TenantId::new().record_id()))
            .bind(("other_dataset", RecordingDatasetId::new().record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            RecordingRepository::new(db.b.clone())
                .reusable_recording_read_grant(&f.scope, &request, id)
                .await
                .unwrap()
                .is_none()
        );
        denied(
            RecordingRepository::new(db.b.clone())
                .create_recording_read_grant(f.draft(class))
                .await,
        );
        db.a.client()
            .query(include_str!("queries/recording_grants/qualify_6.surql"))
            .bind(("recording", f.recordings[1].record_id()))
            .bind(("tenant", f.scope.tenant_id.record_id()))
            .bind(("dataset", f.dataset.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    db.a.client()
        .query(include_str!("queries/recording_grants/qualify_7.surql"))
        .bind(("dataset", f.dataset.record_id()))
        .bind(("tenant", TenantId::new().record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        RecordingRepository::new(db.b.clone())
            .reusable_recording_read_grant(&f.scope, &request, id)
            .await
            .unwrap()
            .is_none()
    );
    denied(
        RecordingRepository::new(db.b.clone())
            .create_recording_read_grant(f.draft(class))
            .await,
    );
    db.a.client()
        .query(include_str!("queries/recording_grants/qualify_8.surql"))
        .bind(("dataset", f.dataset.record_id()))
        .bind(("tenant", f.scope.tenant_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(baseline, count(&db.a).await);

    // Inject a failure during the write. A separate client must observe no partial grant.
    db.a.client()
        .query(include_str!("queries/recording_grants/qualify_9.surql"))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        RecordingRepository::new(db.b.clone())
            .create_recording_read_grant(f.draft(class))
            .await
            .is_err()
    );
    assert_eq!(baseline, count(&db.a).await);
    db.a.client()
        .query(include_str!("queries/recording_grants/qualify_10.surql"))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        RecordingRepository::new(db.b.clone())
            .create_recording_read_grant(f.draft(class))
            .await
            .is_ok()
    );
    assert_eq!(baseline + 1, count(&db.a).await);

    // SQL needs parent authority fields, not a fully decoded Recording model.
    db.a.client()
        .query(include_str!("queries/recording_grants/qualify_11.surql"))
        .bind(("recording", f.recordings[0].record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        RecordingRepository::new(db.b.clone())
            .reusable_recording_read_grant(&f.scope, &request, id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        RecordingRepository::new(db.b.clone())
            .create_recording_read_grant(f.draft(class))
            .await
            .is_ok()
    );
    db.a.client()
        .query(include_str!("queries/recording_grants/qualify_12.surql"))
        .bind(("recording", f.recordings[0].record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        RecordingRepository::new(db.b.clone())
            .reusable_recording_read_grant(&f.scope, &request, id)
            .await
            .unwrap()
            .is_none()
    );
    denied(
        RecordingRepository::new(db.b.clone())
            .create_recording_read_grant(f.draft(class))
            .await,
    );
}
