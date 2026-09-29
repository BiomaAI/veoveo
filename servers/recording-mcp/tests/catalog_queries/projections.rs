//! Projection redemption uses one SQL admission before decoding or filesystem access.
use super::{fixture, identity};
use chrono::{TimeDelta, Utc};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, time::Duration};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_platform_store::{
    RecordingDatasetDraft, RecordingDatasetId, RecordingDraft, RecordingId,
    RecordingProjectionReceiptDraft, RecordingProjectionReceiptId, RecordingReadGrantClass,
    RecordingReadGrantDraft, RecordingReadGrantId,
};
use veoveo_recording_mcp::{RecordingService, service::ProjectionRuntimeLimits};
use veoveo_recording_reader::access::record_uuid;
use veoveo_recording_reader::cache::LayerCacheLimits;
use veoveo_types::{PolicyVersion, WorkContextId};

#[tokio::test]
async fn projection_download_admits_current_authority_and_parents_before_decode() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = fixture::TestDb::new().await;
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
    let dataset =
        db.a.ensure_recording_dataset(RecordingDatasetDraft::installation_default(
            platform.clone(),
            "projection-query",
        ))
        .await
        .unwrap();
    let dataset_id =
        RecordingDatasetId::from_uuid(record_uuid(&dataset.id, RecordingDatasetId::TABLE).unwrap());
    let recording =
        db.a.create_recording(RecordingDraft {
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
    let grant =
        db.a.create_recording_read_grant(RecordingReadGrantDraft {
            identity: platform.clone(),
            authority,
            dataset_id,
            grant_class: RecordingReadGrantClass::AppProjection,
            recording_ids: vec![recording_id],
            catalog_revision: "catalog-1".into(),
            expires_at: Utc::now() + TimeDelta::minutes(5),
        })
        .await
        .unwrap();
    let grant_id = RecordingReadGrantId::from_uuid(
        record_uuid(&grant.id, RecordingReadGrantId::TABLE).unwrap(),
    );
    let reserved =
        db.a.reserve_recording_projection(RecordingProjectionReceiptDraft {
            identity: platform.clone(),
            grant_id,
            caller_idempotency_key: "download".into(),
            manifest_digest: "a".repeat(64),
            query_digest: "b".repeat(64),
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
    db.a.begin_recording_projection(&platform, projection_id)
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
    let baseline =
        db.a.complete_recording_projection(&platform, projection_id, bytes.len() as i64, &digest)
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
    assert_eq!(download.sha256, digest);
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
    db.a.client().query("DEFINE FIELD OVERWRITE manifest_digest ON recording_projection_receipt TYPE any; UPDATE $projection SET manifest_digest = 17 RETURN NONE;")
            .bind(("projection", projection_id.record_id())).await.unwrap().check().unwrap();
    assert!(
        service
            .projection_download(&caller, recording_id, projection_id)
            .await
            .is_err()
    );
    let other_actor = identity("projection-query", "another-reader", &["operations"]);
    let other_tenant = identity("another-tenant", "foreign-reader", &["operations"]);
    let mut other_context = caller.clone();
    other_context.authority.work_context = WorkContextId::new("another-context").unwrap();
    let mut other_policy = caller.clone();
    other_policy.authority.policy_revision = PolicyVersion::new("r2").unwrap();
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
            "UPDATE $projection SET expires_at = time::now() - 1s RETURN NONE;",
        ),
        (
            "failed receipt",
            "UPDATE $projection SET state = 'failed' RETURN NONE;",
        ),
        (
            "cancelled receipt",
            "UPDATE $projection SET state = 'cancelled' RETURN NONE;",
        ),
        (
            "empty recording set",
            "UPDATE $projection SET recordings = [] RETURN NONE;",
        ),
        (
            "extra recording",
            "UPDATE $projection SET recordings += $other_recording RETURN NONE;",
        ),
        (
            "wrong dataset",
            "UPDATE $projection SET dataset = $other_dataset RETURN NONE;",
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
        db.a.client().query("UPDATE $projection CONTENT $baseline RETURN NONE; UPDATE $projection SET manifest_digest = 17 RETURN NONE;")
                .bind(("projection", projection_id.record_id())).bind(("baseline", baseline.clone()))
                .await.unwrap().check().unwrap();
    }
    for (name, sql) in [
        (
            "expired grant",
            "UPDATE $grant SET expires_at = time::now() - 1s RETURN NONE;",
        ),
        (
            "shortened grant",
            "UPDATE $grant SET expires_at = time::now() + 1m RETURN NONE;",
        ),
        (
            "wrong grant class",
            "UPDATE $grant SET grant_class = 'catalog_dataset' RETURN NONE;",
        ),
        (
            "wrong grant actor",
            "UPDATE $grant SET actor = $other_actor RETURN NONE;",
        ),
        (
            "wrong grant tenant",
            "UPDATE $grant SET tenant = $other_tenant RETURN NONE;",
        ),
        (
            "wrong grant context",
            "UPDATE $grant SET work_context = $other_context RETURN NONE;",
        ),
        (
            "wrong grant policy",
            "UPDATE $grant SET policy_revision = 'r2' RETURN NONE;",
        ),
        (
            "wrong grant dataset",
            "UPDATE $grant SET dataset = $other_dataset RETURN NONE;",
        ),
        (
            "wrong grant recordings",
            "UPDATE $grant SET recordings = [$other_recording] RETURN NONE;",
        ),
        (
            "wrong grant catalog",
            "UPDATE $grant SET catalog_revision = 'catalog-2' RETURN NONE;",
        ),
        ("missing grant", "DELETE $grant RETURN NONE;"),
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
            .query("UPSERT $grant CONTENT $baseline RETURN NONE;")
            .bind(("grant", grant_id.record_id()))
            .bind(("baseline", grant.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    db.a.client()
        .query("UPDATE $recording SET labels = ['restricted'] RETURN NONE;")
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
        .query("UPDATE $recording CONTENT $baseline RETURN NONE;")
        .bind(("recording", recording_id.record_id()))
        .bind(("baseline", recording.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.client()
        .query("DELETE $dataset RETURN NONE;")
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
        .query("CREATE $dataset CONTENT $baseline RETURN NONE;")
        .bind(("dataset", dataset_id.record_id()))
        .bind(("baseline", dataset))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.client()
        .query("DELETE $recording RETURN NONE;")
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
        .query("CREATE $recording CONTENT $baseline RETURN NONE;")
        .bind(("recording", recording_id.record_id()))
        .bind(("baseline", recording))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.client()
        .query("UPDATE $projection CONTENT $baseline RETURN NONE;")
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
