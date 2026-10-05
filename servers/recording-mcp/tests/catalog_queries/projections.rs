//! Projection redemption uses one SQL admission before decoding or filesystem access.
use super::{fixture, identity};
use chrono::{TimeDelta, Utc};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, time::Duration};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_recording_mcp::{RecordingService, service::ProjectionRuntimeLimits};
use veoveo_recording_reader::access::record_uuid;
use veoveo_recording_reader::cache::LayerCacheLimits;
use veoveo_recording_store::RecordingRepository;
use veoveo_recording_store::{
    RecordingAccessScope, RecordingDatasetDraft, RecordingDatasetId, RecordingDraft, RecordingId,
    RecordingProjectionReceiptDraft, RecordingProjectionReceiptId, RecordingProjectionRequest,
    RecordingReadGrantClass, RecordingReadGrantDraft, RecordingReadGrantId,
};
use veoveo_types::{PolicyVersion, WorkContextId};

#[tokio::test]
async fn projection_download_admits_current_authority_and_parents_before_decode() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = fixture::TestDb::with_modules(vec![
        veoveo_recording_store::schema::module_setup(
            fixture::module_lanes::execution("recordings").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    tokio::time::timeout(Duration::from_secs(90), assert_download_admission(&db))
        .await
        .expect("projection download admission exceeded 90 seconds");
}

async fn assert_download_admission(db: &fixture::TestDb) {
    let scratch = tempfile::tempdir().unwrap();
    std::fs::create_dir(scratch.path().join("spool")).unwrap();
    let service = RecordingService::new(
        db.b.clone(),
        HttpArtifactPlane::new("http://127.0.0.1:1"),
        scratch.path().join("spool"),
    )
    .unwrap()
    .with_layer_cache(
        scratch.path().join("cache"),
        LayerCacheLimits {
            managed_bytes: 4096,
            minimum_free_bytes: 1,
        },
    )
    .unwrap()
    .with_projection_runtime(ProjectionRuntimeLimits {
        aggregate_scratch_bytes: 4096,
        minimum_free_bytes: 1,
        concurrent_projections: 1,
        maximum_deadline_ms: 1000,
    })
    .unwrap();
    let caller = identity("projection-query", "reader", &["operations"]);
    let platform = service.platform_identity(&caller).await.unwrap();
    let authority = veoveo_recording_hub::invocation_authority_record(&caller.authority);
    let scope = RecordingAccessScope {
        tenant_id: platform.tenant_id,
        actor_id: platform.principal_id,
        work_context_id: veoveo_platform_store::deterministic_work_context_id(
            &platform.tenant_key,
            caller.authority.work_context.as_str(),
        )
        .unwrap(),
        policy_revision: caller.authority.policy_revision.clone(),
        data_labels: caller.actor.data_labels.clone(),
    };
    let dataset = RecordingRepository::new(db.a.clone())
        .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
            platform.clone(),
            "projection-query",
        ))
        .await
        .unwrap();
    let dataset_id =
        RecordingDatasetId::from_uuid(record_uuid(&dataset.id, RecordingDatasetId::TABLE).unwrap());
    let recording = RecordingRepository::new(db.a.clone())
        .create_recording(RecordingDraft {
            identity: platform.clone(),
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
    let recording_id =
        RecordingId::from_uuid(record_uuid(&recording.id, RecordingId::TABLE).unwrap());
    let grant = RecordingRepository::new(db.a.clone())
        .create_recording_read_grant(RecordingReadGrantDraft {
            scope: scope.clone(),
            request: veoveo_recording_store::RecordingReadGrantRequest::new(
                dataset_id,
                RecordingReadGrantClass::AppProjection,
                vec![recording_id],
                "catalog-1",
            )
            .unwrap(),
            expires_at: Utc::now() + TimeDelta::minutes(5),
        })
        .await
        .unwrap();
    let grant_id = RecordingReadGrantId::from_uuid(
        record_uuid(&grant.id, RecordingReadGrantId::TABLE).unwrap(),
    );
    let reserved = RecordingRepository::new(db.a.clone())
        .reserve_recording_projection(RecordingProjectionReceiptDraft {
            scope: scope.clone(),
            request: RecordingProjectionRequest::new(
                dataset_id,
                recording_id,
                "download",
                veoveo_types::Sha256Digest::from_hex("a".repeat(64)).unwrap(),
                veoveo_types::Sha256Digest::from_hex("b".repeat(64)).unwrap(),
            )
            .unwrap(),
            grant_id,
            expires_at: Utc::now() + TimeDelta::minutes(3),
        })
        .await
        .unwrap();
    let projection_id = RecordingProjectionReceiptId::from_uuid(
        record_uuid(&reserved.id, RecordingProjectionReceiptId::TABLE).unwrap(),
    );
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .unwrap()
            .is_none()
    );
    RecordingRepository::new(db.a.clone())
        .begin_recording_projection(&scope, recording_id, projection_id)
        .await
        .unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .unwrap()
            .is_none()
    );
    // This fixture qualifies download integrity, not Arrow generation or decoding.
    let bytes = b"projection download integrity fixture";
    let digest = hex::encode(Sha256::digest(bytes));
    let baseline = RecordingRepository::new(db.a.clone())
        .complete_recording_projection(
            &scope,
            recording_id,
            projection_id,
            bytes.len() as u64,
            &veoveo_types::Sha256Digest::from_hex(&digest).unwrap(),
        )
        .await
        .unwrap();
    let path = scratch
        .path()
        .join("cache/projections")
        .join(format!("{projection_id}.arrow"));
    std::fs::write(&path, bytes).unwrap();
    let download = service
        .projection_download(&caller, recording_id, projection_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(download.path, path);
    assert_eq!(download.byte_len, bytes.len() as u64);
    assert_eq!(download.sha256.hex(), digest);
    assert!(
        service
            .projection_download(&caller, RecordingId::new(), projection_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        service
            .projection_download(&caller, recording_id, RecordingProjectionReceiptId::new())
            .await
            .unwrap()
            .is_none()
    );

    // An unrelated malformed field proves denial happens before typed decoding.
    // Only this disposable fixture changes the schema to inject corrupt data.
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission.surql"
        ))
        .bind(("projection", projection_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .is_err()
    );
    let other_actor = identity("projection-query", "another-reader", &["operations"]);
    let other_tenant = identity("another-tenant", "foreign-reader", &["operations"]);
    let mut other_context = caller.clone();
    other_context.authority.work_context = WorkContextId::parse("another-context").unwrap();
    let mut other_policy = caller.clone();
    other_policy.authority.policy_revision = PolicyVersion::parse("r2").unwrap();
    let mut no_clearance = caller.clone();
    no_clearance.actor.data_labels.clear();
    for (name, denied) in [
        ("actor", other_actor),
        ("tenant", other_tenant),
        ("context", other_context),
        ("policy", other_policy),
        ("clearance", no_clearance),
    ] {
        assert!(
            service
                .projection_download(&denied, recording_id, projection_id)
                .await
                .unwrap_or_else(|error| panic!("{name}: {error:#}"))
                .is_none(),
            "{name}"
        );
    }

    for (name, sql) in [
        (
            "expired receipt",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_2.surql"
            ),
        ),
        (
            "failed receipt",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_3.surql"
            ),
        ),
        (
            "cancelled receipt",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_4.surql"
            ),
        ),
        (
            "empty recording set",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_5.surql"
            ),
        ),
        (
            "extra recording",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_6.surql"
            ),
        ),
        (
            "wrong dataset",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_7.surql"
            ),
        ),
    ] {
        db.a.client()
            .query(sql)
            .bind(("projection", projection_id.record_id()))
            .bind(("other_recording", RecordingId::new().record_id()))
            .bind(("other_dataset", RecordingDatasetId::new().record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            service
                .projection_download(&caller, recording_id, projection_id)
                .await
                .unwrap()
                .is_none(),
            "{name}"
        );
        db.a.client()
            .query(include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_8.surql"
            ))
            .bind(("projection", projection_id.record_id()))
            .bind(("baseline", baseline.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    for (name, sql) in [
        (
            "expired grant",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_9.surql"
            ),
        ),
        (
            "shortened grant",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_10.surql"
            ),
        ),
        (
            "wrong grant class",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_11.surql"
            ),
        ),
        (
            "wrong grant actor",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_12.surql"
            ),
        ),
        (
            "wrong grant tenant",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_13.surql"
            ),
        ),
        (
            "wrong grant context",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_14.surql"
            ),
        ),
        (
            "wrong grant policy",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_15.surql"
            ),
        ),
        (
            "wrong grant dataset",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_16.surql"
            ),
        ),
        (
            "wrong grant recordings",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_17.surql"
            ),
        ),
        (
            "wrong grant catalog",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_18.surql"
            ),
        ),
        (
            "missing grant",
            include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_19.surql"
            ),
        ),
    ] {
        db.a.client()
            .query(sql)
            .bind(("grant", grant_id.record_id()))
            .bind((
                "other_actor",
                veoveo_platform_store::PrincipalId::new().record_id(),
            ))
            .bind((
                "other_tenant",
                veoveo_platform_store::TenantId::new().record_id(),
            ))
            .bind((
                "other_context",
                veoveo_platform_store::WorkContextId::new().record_id(),
            ))
            .bind(("other_dataset", RecordingDatasetId::new().record_id()))
            .bind(("other_recording", RecordingId::new().record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            service
                .projection_download(&caller, recording_id, projection_id)
                .await
                .unwrap()
                .is_none(),
            "{name}"
        );
        db.a.client()
            .query(include_str!(
                "../queries/catalog_queries/projections/assert_download_admission_20.surql"
            ))
            .bind(("grant", grant_id.record_id()))
            .bind(("baseline", grant.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission_21.surql"
        ))
        .bind(("recording", recording_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .unwrap()
            .is_none()
    );
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission_22.surql"
        ))
        .bind(("recording", recording_id.record_id()))
        .bind(("baseline", recording.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission_23.surql"
        ))
        .bind(("dataset", dataset_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .unwrap()
            .is_none()
    );
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission_24.surql"
        ))
        .bind(("dataset", dataset_id.record_id()))
        .bind(("baseline", dataset))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission_25.surql"
        ))
        .bind(("recording", recording_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .unwrap()
            .is_none()
    );
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission_26.surql"
        ))
        .bind(("recording", recording_id.record_id()))
        .bind(("baseline", recording))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.client()
        .query(include_str!(
            "../queries/catalog_queries/projections/assert_download_admission_27.surql"
        ))
        .bind(("projection", projection_id.record_id()))
        .bind(("baseline", baseline))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .unwrap()
            .is_some()
    );
    std::fs::write(&path, vec![b'x'; bytes.len()]).unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .is_err()
    );
    std::fs::write(&path, b"short").unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .is_err()
    );
}
