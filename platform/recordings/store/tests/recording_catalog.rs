use chrono::{TimeDelta, Utc};
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_platform_store::*;
use veoveo_recording_store::*;
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "recording_catalog/ingest.rs"]
mod recording_ingest;
fn artifact_authority(identity: &PlatformIdentity) -> InvocationAuthorityRecord {
    InvocationAuthorityRecord {
        context_key: "operations".into(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: "r1".into(),
        owner_kind: ArtifactGrantSubjectKind::Principal,
        owner_key: identity.principal_key.clone(),
        initial_grants: vec![WorkContextInitialGrantRecord {
            subject_kind: ArtifactGrantSubjectKind::Principal,
            subject_key: identity.principal_key.clone(),
            permission: GrantPermission::Admin,
        }],
        classification: None,
        data_labels: Vec::new(),
        invocation_mode: InvocationMode::Direct,
        initiator_key: Some(identity.principal_key.clone()),
        delegation_id: None,
    }
}

#[tokio::test]
async fn recording_catalog_commits_layers_and_governed_authority_atomically() {
    let db = fixture::TestDb::with_modules(vec![
        veoveo_recording_store::schema::module_setup(
            fixture::module_lanes::execution("recordings").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    tokio::time::timeout(
        std::time::Duration::from_secs(90),
        qualify_recording_catalog(&db.a, &db.b),
    )
    .await
    .expect("Recording catalog qualification exceeded 90 seconds");
    let changes = db
        .committed(veoveo_platform_store::ObservationTable::new(
            veoveo_modules::TableName::new("recording").unwrap(),
            veoveo_platform_store::ObservationReplay::Changefeed(
                veoveo_platform_store::ChangefeedRetention::from_days(30).unwrap(),
            ),
        ))
        .await;
    assert!(changes.iter().any(|row| row["state"] == "sealed"));
}

async fn qualify_recording_catalog(platform: &PlatformStore, read_platform: &PlatformStore) {
    let store = RecordingRepository::new(platform.clone());
    let reader = RecordingRepository::new(read_platform.clone());
    let identity = platform
        .ensure_identity(
            "tenant-recording",
            "recording-hub",
            "https://veoveo.local/services",
            "recording-hub",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let dataset = store
        .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
            identity.clone(),
            "world",
        ))
        .await
        .unwrap();
    let dataset_id =
        veoveo_recording_store::RecordingDatasetId::from_uuid(record_uuid(&dataset.id));
    let retried_dataset = store
        .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
            identity.clone(),
            "world",
        ))
        .await
        .unwrap();
    assert_eq!(dataset.id, retried_dataset.id);

    let recording = store
        .create_recording(RecordingDraft {
            identity: identity.clone(),
            authority: artifact_authority(&identity),
            dataset_id,
            application_id: "sensor-suite".into(),
            recording_key: "run-42".into(),
            classification: "restricted".into(),
            labels: vec!["operations".into(), "restricted".into()],
            metadata: BTreeMap::new(),
            started_at: Utc::now(),
        })
        .await
        .unwrap();
    let recording_id = RecordingId::from_uuid(record_uuid(&recording.id));
    let first = store
        .open_recording_layer(
            RecordingLayerDraft::capture(
                identity.clone(),
                recording_id,
                0,
                "world/2026-07-09/run-42.rrd".into(),
                Some(Utc::now()),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let second = store
        .open_recording_layer(
            RecordingLayerDraft::capture(
                identity.clone(),
                recording_id,
                1,
                "world/2026-07-09/run-42.r1.rrd".into(),
                Some(Utc::now()),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let first_id = RecordingLayerId::from_uuid(record_uuid(&first.id));
    let second_id = RecordingLayerId::from_uuid(record_uuid(&second.id));
    assert_eq!(
        store
            .stage_recording_layer(
                &identity,
                first_id,
                128,
                10,
                &veoveo_types::Sha256Digest::from_hex("c".repeat(64)).unwrap(),
                Some("0.38.1"),
                Some(&veoveo_types::Sha256Digest::from_hex("a".repeat(64)).unwrap()),
                Some(Utc::now())
            )
            .await
            .unwrap()
            .state,
        RecordingLayerState::Staged
    );
    store
        .stage_recording_layer(
            &identity,
            second_id,
            128,
            20,
            &veoveo_types::Sha256Digest::from_hex("d".repeat(64)).unwrap(),
            Some("0.38.1"),
            Some(&veoveo_types::Sha256Digest::from_hex("b".repeat(64)).unwrap()),
            Some(Utc::now()),
        )
        .await
        .unwrap();

    let first_artifact = ArtifactId::new();
    let second_artifact = ArtifactId::new();
    let manifest_artifact = ArtifactId::new();
    for (artifact_id, hash, filename) in [
        (first_artifact, "c".repeat(64), "run-42.rrd"),
        (second_artifact, "d".repeat(64), "run-42.r1.rrd"),
        (manifest_artifact, "e".repeat(64), "run-42.recording.json"),
    ] {
        store
            .platform()
            .create_artifact_occurrence(ArtifactOccurrenceDraft {
                artifact_id,
                identity: identity.clone(),
                authority: artifact_authority(&identity),
                owner: identity.principal_id.record_id(),
                initial_grants: vec![owner_grant(artifact_id, &identity)],
                sha256: hash,
                byte_len: 128,
                object_key: format!("recording-test/{artifact_id}"),
                media_type: "application/octet-stream".into(),
                filename: Some(filename.into()),
                classification: "restricted".into(),
                labels: vec!["operations".into(), "restricted".into()],
                metadata: BTreeMap::new(),
                retention_expires_at: None,
            })
            .await
            .unwrap();
    }
    store
        .commit_recording_layer(&identity, first_id, first_artifact)
        .await
        .unwrap();
    store
        .commit_recording_layer(&identity, second_id, second_artifact)
        .await
        .unwrap();
    assert!(matches!(
        store
            .commit_recording_layer(&identity, second_id, first_artifact)
            .await,
        Err(RecordingStoreError::RecordingLayerConflict { .. })
    ));
    let dataset_after_capture = store
        .recording_dataset(identity.tenant_id, dataset_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dataset_after_capture.revision, 2);

    let capture_ended_at = Utc::now();
    let ready = store
        .finish_recording(&identity, recording_id, capture_ended_at)
        .await
        .unwrap();
    assert_eq!(ready.state, RecordingState::Ready);
    assert_eq!(ready.ended_at, Some(capture_ended_at));
    assert_eq!(
        store
            .begin_recording_seal(&identity, recording_id, None)
            .await
            .unwrap()
            .state,
        RecordingState::Sealing
    );

    let properties = store
        .open_recording_layer(RecordingLayerDraft {
            identity: identity.clone(),
            recording_id,
            layer_name: "properties".into(),
            kind: RecordingLayerKind::Properties,
            ordinal: None,
            staging_path: Some("world/2026-07-09/run-42.properties.rrd".into()),
            start_time: None,
        })
        .await
        .unwrap();
    let properties_id = RecordingLayerId::from_uuid(record_uuid(&properties.id));
    store
        .stage_recording_layer(
            &identity,
            properties_id,
            128,
            1,
            &veoveo_types::Sha256Digest::from_hex("f".repeat(64)).unwrap(),
            Some("0.38.1"),
            Some(&veoveo_types::Sha256Digest::from_hex("9".repeat(64)).unwrap()),
            None,
        )
        .await
        .unwrap();
    let properties_artifact = ArtifactId::new();
    store
        .platform()
        .create_artifact_occurrence(ArtifactOccurrenceDraft {
            artifact_id: properties_artifact,
            identity: identity.clone(),
            authority: artifact_authority(&identity),
            owner: identity.principal_id.record_id(),
            initial_grants: vec![owner_grant(properties_artifact, &identity)],
            sha256: "f".repeat(64),
            byte_len: 128,
            object_key: format!("recording-test/{properties_artifact}"),
            media_type: "application/octet-stream".into(),
            filename: Some("run-42.properties.rrd".into()),
            classification: "restricted".into(),
            labels: vec!["operations".into(), "restricted".into()],
            metadata: BTreeMap::new(),
            retention_expires_at: None,
        })
        .await
        .unwrap();
    store
        .commit_recording_layer(&identity, properties_id, properties_artifact)
        .await
        .unwrap();
    store
        .stage_recording_manifest(&identity, recording_id, manifest_artifact)
        .await
        .unwrap();
    let sealed = store
        .complete_recording_seal(RecordingSeal {
            identity: identity.clone(),
            recording_id,
            task_id: None,
            manifest_artifact_id: manifest_artifact,
            sealed_at: Utc::now(),
        })
        .await
        .unwrap();
    assert_eq!(sealed.state, RecordingState::Sealed);
    assert_eq!(sealed.ended_at, Some(capture_ended_at));
    assert!(sealed.sealed_at.is_some());
    assert_eq!(
        sealed.manifest_artifact,
        Some(manifest_artifact.record_id())
    );
    let layers = store
        .recording_layers(identity.tenant_id, recording_id, 10)
        .await
        .unwrap();
    assert!(layers.iter().all(|layer| {
        layer.state == RecordingLayerState::Committed && layer.artifact.is_some()
    }));
    assert!(layers.iter().all(|layer| layer.staging_path.is_none()));

    let scope = veoveo_recording_store::RecordingAccessScope {
        tenant_id: identity.tenant_id,
        actor_id: identity.principal_id,
        work_context_id: deterministic_work_context_id(&identity.tenant_key, "operations").unwrap(),
        policy_revision: veoveo_types::PolicyVersion::parse("r1").unwrap(),
        data_labels: ["operations", "restricted"]
            .map(|label| veoveo_types::DataLabelId::parse(label).unwrap())
            .into_iter()
            .collect(),
    };
    let grant_expires_at = Utc::now() + TimeDelta::minutes(5);
    let grant_request = veoveo_recording_store::RecordingReadGrantRequest::new(
        dataset_id,
        RecordingReadGrantClass::AppProjection,
        vec![recording_id, recording_id],
        "3",
    )
    .unwrap();
    let grant = store
        .create_recording_read_grant(RecordingReadGrantDraft {
            scope: scope.clone(),
            request: grant_request.clone(),
            expires_at: grant_expires_at,
        })
        .await
        .unwrap();
    assert_eq!(grant.recordings, vec![recording_id.record_id()]);
    let grant_id = veoveo_recording_store::RecordingReadGrantId::from_uuid(record_uuid(&grant.id));
    assert_eq!(
        Some(grant),
        reader
            .reusable_recording_read_grant(&scope, &grant_request, grant_id)
            .await
            .unwrap()
    );
    let projection_draft = RecordingProjectionReceiptDraft {
        scope: scope.clone(),
        request: veoveo_recording_store::RecordingProjectionRequest::new(
            dataset_id,
            recording_id,
            "projection-1",
            veoveo_types::Sha256Digest::from_hex("1".repeat(64)).unwrap(),
            veoveo_types::Sha256Digest::from_hex("2".repeat(64)).unwrap(),
        )
        .unwrap(),
        grant_id,
        expires_at: Utc::now() + TimeDelta::minutes(1),
    };
    let projection = store
        .reserve_recording_projection(projection_draft.clone())
        .await
        .unwrap();
    let retried_projection = store
        .reserve_recording_projection(projection_draft)
        .await
        .unwrap();
    assert_eq!(projection.id, retried_projection.id);
    let projection_id = veoveo_recording_store::RecordingProjectionReceiptId::from_uuid(
        record_uuid(&projection.id),
    );
    assert_eq!(
        store
            .begin_recording_projection(&scope, recording_id, projection_id)
            .await
            .unwrap()
            .state,
        RecordingProjectionState::Materializing
    );
    assert_eq!(
        store
            .complete_recording_projection(
                &scope,
                recording_id,
                projection_id,
                512,
                &veoveo_types::Sha256Digest::from_hex("3".repeat(64)).unwrap()
            )
            .await
            .unwrap()
            .state,
        RecordingProjectionState::Ready
    );
    let cleanup = store
        .cleanup_expired_recording_catalog_authority(grant_expires_at + TimeDelta::seconds(1))
        .await
        .unwrap();
    assert_eq!(cleanup.projection_receipts, 1);
    assert_eq!(cleanup.read_grants, 1);
    let other = platform
        .ensure_identity(
            "other-tenant",
            "reader",
            "https://idp.example.com",
            "reader",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    assert!(
        store
            .recording(other.tenant_id, recording_id)
            .await
            .unwrap()
            .is_none()
    );
}

fn record_uuid(record: &veoveo_platform_store::RecordId) -> Uuid {
    match &record.key {
        RecordIdKey::Uuid(value) => Uuid::parse_str(&value.to_string()).unwrap(),
        RecordIdKey::String(value) => Uuid::parse_str(value).unwrap(),
        other => panic!("expected UUID record key, got {other:?}"),
    }
}

fn owner_grant(artifact_id: ArtifactId, identity: &PlatformIdentity) -> ArtifactGrantDraft {
    ArtifactGrantDraft {
        artifact_id,
        subject: identity.principal_id.record_id(),
        subject_kind: ArtifactGrantSubjectKind::Principal,
        subject_key: identity.principal_key.clone(),
        permission: GrantPermission::Admin,
        labels: Vec::new(),
        expires_at: None,
        created_by: identity.principal_id,
    }
}
