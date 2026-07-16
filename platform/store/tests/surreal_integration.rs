use std::collections::BTreeMap;

use chrono::{TimeDelta, Utc};
use secrecy::SecretString;
use uuid::Uuid;
use veoveo_platform_store::{
    ArtifactId, ArtifactOccurrenceDraft, ArtifactReleaseState, ArtifactShareLinkDraft,
    ArtifactWriteCapabilityDraft, ArtifactWriteCapabilityId, ArtifactWriteCapabilityRecord,
    ArtifactWriteRedemptionId, ChangefeedCursor, CoordinateFrameDraft, CoordinateOperationDraft,
    GatewayReplayKind, GatewayReplayRecord, MapReleaseDraft, MapReleaseState, OpenObject,
    OutboxDraft, PlatformStore, PlatformTable, PrincipalKind, RecordIdKey, RecordingDraft,
    RecordingId, RecordingSeal, RecordingState, SegmentDraft, SegmentId, SegmentSealBinding,
    SegmentState, ShareLinkId, StoreConfig, StoreCredentials, StoreError, TaskId,
    TimeAuthorityReleaseDraft, TimeAuthorityReleaseState, TimeDatasetKind, TimeSourceDraft,
    gateway_replay_record_id,
};

#[tokio::test]
async fn time_authority_activation_retires_the_previous_release_atomically() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &endpoint,
            "veoveo_integration",
            format!("time_activation_test_{}", Uuid::now_v7().simple()),
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    let identity = store
        .ensure_identity(
            "tenant-time",
            "time-admin",
            "https://veoveo.local/services",
            "time-admin",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let source_key = format!("time-source-{}", Uuid::now_v7());
    store
        .create_time_source(TimeSourceDraft {
            identity: identity.clone(),
            source_key: source_key.clone(),
            name: "IANA leap seconds".to_owned(),
            dataset_kind: TimeDatasetKind::LeapSeconds,
            source_url: "https://example.com/leap-seconds.list".to_owned(),
            expected_content_type: "text/plain".to_owned(),
            enabled: true,
            canonical_json: serde_json::json!({"source_id": source_key}).to_string(),
        })
        .await
        .unwrap();
    let create_release = |release_key: String, digest: String| TimeAuthorityReleaseDraft {
        identity: identity.clone(),
        release_key,
        source_key: source_key.clone(),
        dataset_kind: TimeDatasetKind::LeapSeconds,
        state: TimeAuthorityReleaseState::Staged,
        version_label: format!("iana-{}", &digest[..12]),
        source_url: "https://example.com/leap-seconds.list".to_owned(),
        source_digest_sha256: digest,
        artifact_path: "/var/lib/veoveo/time/releases/test/leap-seconds.list".to_owned(),
        retrieved_at: Utc::now(),
        validated_at: Utc::now(),
        canonical_json: serde_json::json!({"state": "staged"}).to_string(),
    };

    let first_key = format!("time-release-{}", Uuid::now_v7());
    store
        .create_time_authority_release(create_release(first_key.clone(), "a".repeat(64)))
        .await
        .unwrap();
    let first = store
        .activate_time_authority_release(
            &identity,
            &first_key,
            1,
            0,
            serde_json::json!({"state": "active"}).to_string(),
        )
        .await
        .unwrap();
    assert_eq!(first.state, TimeAuthorityReleaseState::Active);

    let second_key = format!("time-release-{}", Uuid::now_v7());
    store
        .create_time_authority_release(create_release(second_key.clone(), "b".repeat(64)))
        .await
        .unwrap();
    let second = store
        .activate_time_authority_release(
            &identity,
            &second_key,
            1,
            1,
            serde_json::json!({"state": "active"}).to_string(),
        )
        .await
        .unwrap();
    assert_eq!(second.state, TimeAuthorityReleaseState::Active);
    let retired = store
        .time_authority_release(identity.tenant_id, &first_key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retired.state, TimeAuthorityReleaseState::Retired);
    assert_eq!(retired.record_version, 3);
    let pointer = store
        .active_time_authority(identity.tenant_id, TimeDatasetKind::LeapSeconds)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pointer.release_key, second_key);
    assert_eq!(
        pointer.previous_release_key.as_deref(),
        Some(first_key.as_str())
    );
    assert_eq!(pointer.record_version, 2);
}

#[tokio::test]
async fn map_release_activation_is_atomic_and_version_guarded() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &endpoint,
            "veoveo_integration",
            format!("map_activation_test_{}", Uuid::now_v7().simple()),
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    let identity = store
        .ensure_identity(
            "tenant-map",
            "map-admin",
            "https://veoveo.local/services",
            "map-admin",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let dataset_key = format!("dataset-{}", Uuid::now_v7());
    let source_key = format!("source-{}", Uuid::now_v7());
    let create_release = |release_key: String| MapReleaseDraft {
        identity: identity.clone(),
        release_key,
        dataset_key: dataset_key.clone(),
        source_key: source_key.clone(),
        state: MapReleaseState::Staged,
        version_label: format!("sha256:{}", "a".repeat(64)),
        source_digest_sha256: "a".repeat(64),
        valid_from: Utc::now(),
        valid_until: None,
        canonical_json: serde_json::json!({ "state": "staged" }).to_string(),
    };

    let first_key = format!("release-{}", Uuid::now_v7());
    store
        .create_map_release(create_release(first_key.clone()))
        .await
        .unwrap();
    let first = store
        .activate_map_release(
            &identity,
            &dataset_key,
            &first_key,
            None,
            1,
            serde_json::json!({ "state": "active" }).to_string(),
        )
        .await
        .unwrap();
    assert_eq!(first.state, MapReleaseState::Active);
    assert_eq!(first.record_version, 2);
    assert_eq!(
        store
            .active_map_release(identity.tenant_id, &dataset_key)
            .await
            .unwrap()
            .unwrap()
            .record_version,
        1
    );

    let second_key = format!("release-{}", Uuid::now_v7());
    store
        .create_map_release(create_release(second_key.clone()))
        .await
        .unwrap();
    let conflict = store
        .activate_map_release(
            &identity,
            &dataset_key,
            &second_key,
            Some(1),
            2,
            serde_json::json!({ "state": "active" }).to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(conflict, StoreError::MapRecordConflict { .. }),
        "unexpected activation error: {conflict:?}"
    );
    let second = store
        .map_release(identity.tenant_id, &second_key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.state, MapReleaseState::Staged);
    assert_eq!(second.record_version, 1);
    let pointer = store
        .active_map_release(identity.tenant_id, &dataset_key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pointer.release_key, first_key);
    assert_eq!(pointer.record_version, 1);
}

#[tokio::test]
async fn coordinate_frames_and_operations_are_durable_and_idempotent() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &endpoint,
            "veoveo_integration",
            format!("coordinates_test_{}", Uuid::now_v7().simple()),
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    let identity = store
        .ensure_identity(
            "tenant-coordinates",
            "coordinate-user",
            "https://idp.example.com",
            "coordinate-subject",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    let frame = store
        .create_coordinate_frame(CoordinateFrameDraft {
            identity: identity.clone(),
            frame_key: "ENU:integration".to_owned(),
            display_name: "Integration local frame".to_owned(),
            definition: OpenObject::new(BTreeMap::from([(
                "frame_id".to_owned(),
                serde_json::json!("ENU:integration"),
            )])),
            proj_pipeline: None,
            classification: "gateway_labels".to_owned(),
            labels: vec!["cui".to_owned()],
        })
        .await
        .unwrap();
    assert_eq!(frame.frame_key, "ENU:integration");
    assert_eq!(
        store
            .list_coordinate_frames(identity.tenant_id)
            .await
            .unwrap()
            .len(),
        1
    );

    let operation_key = format!("op-{}", Uuid::now_v7());
    let created_at = Utc::now();
    let draft = CoordinateOperationDraft {
        identity: identity.clone(),
        task_id: None,
        operation_key: operation_key.clone(),
        kind: "frame_conversion".to_owned(),
        provenance: OpenObject::new(BTreeMap::from([(
            "operation_id".to_owned(),
            serde_json::json!(operation_key),
        )])),
        classification: "gateway_labels".to_owned(),
        labels: vec!["cui".to_owned()],
        created_at,
    };
    let first = store
        .upsert_coordinate_operation(draft.clone())
        .await
        .unwrap();
    let replay = store.upsert_coordinate_operation(draft).await.unwrap();
    assert_eq!(first.id, replay.id);
    assert_eq!(first.operation_key, operation_key);
    assert!(
        store
            .coordinate_operation(identity.tenant_id, &operation_key)
            .await
            .unwrap()
            .is_some()
    );
}

/// Run explicitly with:
/// `VEOVEO_SURREAL_INTEGRATION=1 VEOVEO_SURREAL_URL=ws://127.0.0.1:8000 cargo test -p veoveo-platform-store --test surreal_integration`
#[tokio::test]
async fn applies_schema_to_surrealdb_3_2() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let database = format!("platform_test_{}", Uuid::now_v7().simple());
    let config = StoreConfig::builder(
        &endpoint,
        "veoveo_integration",
        database.clone(),
        StoreCredentials::root(username, SecretString::from(password)),
    )
    .migrate_on_connect(true)
    .build()
    .unwrap();

    let store = PlatformStore::connect(config).await.unwrap();
    let mut original_session = store
        .client()
        .query("RETURN session::id();")
        .await
        .unwrap()
        .check()
        .unwrap();
    let original_session: surrealdb::types::Value = original_session.take(0).unwrap();
    let cloned_store = store.clone();
    let mut cloned_session = cloned_store
        .client()
        .query("RETURN session::id();")
        .await
        .unwrap()
        .check()
        .unwrap();
    let cloned_session: surrealdb::types::Value = cloned_session.take(0).unwrap();
    assert_eq!(
        cloned_session, original_session,
        "PlatformStore clones must share one authenticated SurrealDB session"
    );

    let status = store.schema_status().await.unwrap();
    assert!(status.is_current(), "{status:?}");
    let second_pass = store.migrate().await.unwrap();
    assert!(second_pass.applied_versions.is_empty());

    let mut response = store.client().query("INFO FOR DB;").await.unwrap();
    let info: surrealdb::types::Value = response.take(0).unwrap();
    let rendered = format!("{info:?}");
    for table in [
        "task",
        "artifact_occurrence",
        "recording",
        "time_authority_release",
        "time_temporal_event",
        "outbox_event",
    ] {
        assert!(rendered.contains(table), "missing {table} in INFO FOR DB");
    }

    let event = store
        .append_outbox(OutboxDraft::now(
            None,
            "integration_test",
            "schema",
            "integration.schema_ready",
            1,
            OpenObject::default(),
        ))
        .await
        .unwrap();
    assert!(event.sequence > 0);

    let page = store.read_outbox(0, 10).await.unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.next_sequence, event.sequence);

    store
        .checkpoint_outbox("integration-projection", event.sequence)
        .await
        .unwrap();
    assert_eq!(
        store
            .outbox_checkpoint("integration-projection")
            .await
            .unwrap(),
        event.sequence
    );
    store
        .checkpoint_outbox("integration-projection", event.sequence - 1)
        .await
        .unwrap();
    assert_eq!(
        store
            .outbox_checkpoint("integration-projection")
            .await
            .unwrap(),
        event.sequence,
        "checkpoint must not move backwards"
    );

    let changes = store
        .replay_changes(PlatformTable::OutboxEvent, ChangefeedCursor::initial(), 100)
        .await
        .unwrap();
    assert!(!changes.is_empty());

    let runtime_password = SecretString::from("runtime-integration-password");
    store
        .replace_database_editor("veoveo_runtime", &runtime_password)
        .await
        .unwrap();
    let runtime = PlatformStore::connect(
        StoreConfig::builder(
            endpoint,
            "veoveo_integration",
            database,
            StoreCredentials::database("veoveo_runtime", runtime_password),
        )
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    runtime.healthcheck().await.unwrap();
    assert!(matches!(
        runtime.migrate().await,
        Err(StoreError::RootCredentialsRequired { .. })
    ));
}

#[tokio::test]
async fn gateway_replay_claim_is_atomic_across_store_instances() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let database = format!("gateway_replay_test_{}", Uuid::now_v7().simple());
    let config = StoreConfig::builder(
        &endpoint,
        "veoveo_integration",
        database,
        StoreCredentials::root(username, SecretString::from(password)),
    )
    .migrate_on_connect(true)
    .build()
    .unwrap();
    let first = PlatformStore::connect(config.clone()).await.unwrap();
    let second = PlatformStore::connect(config).await.unwrap();
    let now = Utc::now();
    let record = GatewayReplayRecord {
        id: gateway_replay_record_id(
            GatewayReplayKind::ClientAssertion,
            "authorization-server",
            "client",
            "jwt-id",
        ),
        kind: GatewayReplayKind::ClientAssertion,
        authorization_server: "authorization-server".to_owned(),
        client_id: "client".to_owned(),
        jwt_id: "jwt-id".to_owned(),
        seen_at: now,
        expires_at: now + TimeDelta::minutes(5),
    };

    let (left, right) = tokio::join!(
        first.register_gateway_replay_id(record.clone(), now),
        second.register_gateway_replay_id(record, now),
    );
    let claims = usize::from(left.unwrap()) + usize::from(right.unwrap());
    assert_eq!(claims, 1, "exactly one concurrent replay claim must win");
}

#[tokio::test]
async fn artifact_plane_counters_and_occurrence_dedup_are_durable() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let database = format!("artifact_test_{}", Uuid::now_v7().simple());
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &endpoint,
            "veoveo_integration",
            database,
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();

    let identity = store
        .ensure_identity(
            "tenant-a",
            "alice",
            "https://idp.example.com",
            "alice-subject",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    let first_id = ArtifactId::new();
    let artifact = store
        .create_artifact_occurrence(ArtifactOccurrenceDraft {
            artifact_id: first_id,
            identity: identity.clone(),
            sha256: "a".repeat(64),
            byte_len: 4,
            object_key: "tenants/tenant-a/blobs/opaque-a".into(),
            media_type: "application/octet-stream".into(),
            filename: Some("result.bin".into()),
            classification: String::new(),
            labels: vec![],
            metadata: BTreeMap::new(),
            retention_expires_at: None,
        })
        .await
        .unwrap();
    let second = store
        .create_artifact_occurrence(ArtifactOccurrenceDraft {
            artifact_id: ArtifactId::new(),
            identity: identity.clone(),
            sha256: "a".repeat(64),
            byte_len: 4,
            object_key: "tenants/tenant-a/blobs/opaque-a".into(),
            media_type: "application/octet-stream".into(),
            filename: None,
            classification: String::new(),
            labels: vec![],
            metadata: BTreeMap::new(),
            retention_expires_at: None,
        })
        .await
        .unwrap();
    assert_ne!(artifact.occurrence.id, second.occurrence.id);
    assert_eq!(artifact.blob.id, second.blob.id);
    assert_eq!(artifact.grants[0].subject_key, "alice");
    let visible = store
        .artifact_ids_for_subjects(
            identity.tenant_id,
            vec![identity.principal_id.record_id()],
            None,
            10,
        )
        .await
        .unwrap();
    assert_eq!(visible.len(), 2);
    assert!(visible.contains(&first_id));
    assert!(
        store
            .read_outbox(0, 100)
            .await
            .unwrap()
            .events
            .iter()
            .filter(|event| event.event_type == "artifact.created")
            .count()
            >= 2
    );

    let capability_id = ArtifactWriteCapabilityId::new();
    let capability_task_id = TaskId::new().to_string();
    store
        .create_artifact_write_capability(ArtifactWriteCapabilityDraft {
            capability_id,
            identity: identity.clone(),
            profile_key: "operator".into(),
            server_key: "media".into(),
            task_id: capability_task_id.clone(),
            owner_kind: PrincipalKind::User,
            owner_issuer: "https://idp.example.com".into(),
            owner_subject: "alice-subject".into(),
            token_hash: "b".repeat(64),
            labels: vec!["cui".into()],
            max_artifact_count: 1,
            max_total_bytes: 4,
            expires_at: Utc::now() + TimeDelta::minutes(5),
        })
        .await
        .unwrap();
    let proposed = first_id;
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                capability_id,
                &"b".repeat(64),
                &TaskId::new().to_string(),
                "media:wrong:output:0",
                &"d".repeat(64),
                4,
                &["cui".into()],
                proposed,
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                capability_id,
                &"0".repeat(64),
                &capability_task_id,
                "media:wrong-token:output:0",
                &"d".repeat(64),
                4,
                &["cui".into()],
                proposed,
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                capability_id,
                &"b".repeat(64),
                &capability_task_id,
                "media:wrong-label:output:0",
                &"d".repeat(64),
                4,
                &["restricted".into()],
                proposed,
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));
    let reserved = store
        .reserve_artifact_write_capability(
            capability_id,
            &"b".repeat(64),
            &capability_task_id,
            "media:task:output:0",
            &"d".repeat(64),
            4,
            &["cui".into()],
            proposed,
        )
        .await
        .unwrap();
    for (token, task, labels) in [
        (
            "0".repeat(64),
            capability_task_id.clone(),
            vec!["cui".into()],
        ),
        (
            "b".repeat(64),
            TaskId::new().to_string(),
            vec!["cui".into()],
        ),
        (
            "b".repeat(64),
            capability_task_id.clone(),
            vec!["restricted".into()],
        ),
    ] {
        assert!(matches!(
            store
                .reserve_artifact_write_capability(
                    capability_id,
                    &token,
                    &task,
                    "media:task:output:0",
                    &"d".repeat(64),
                    4,
                    &labels,
                    ArtifactId::new(),
                )
                .await,
            Err(StoreError::ArtifactWriteDenied)
        ));
    }
    let retry = store
        .reserve_artifact_write_capability(
            capability_id,
            &"b".repeat(64),
            &capability_task_id,
            "media:task:output:0",
            &"d".repeat(64),
            4,
            &["cui".into()],
            ArtifactId::new(),
        )
        .await
        .unwrap();
    assert_eq!(retry.redemption.id, reserved.redemption.id);
    assert_eq!(retry.redemption.artifact, reserved.redemption.artifact);
    let mismatched_after_stage = store
        .reserve_artifact_write_capability(
            capability_id,
            &"b".repeat(64),
            &capability_task_id,
            "media:task:output:0",
            &"e".repeat(64),
            4,
            &["cui".into()],
            ArtifactId::new(),
        )
        .await
        .unwrap();
    assert!(!mismatched_after_stage.request_matches);
    assert_eq!(
        mismatched_after_stage.redemption.artifact,
        first_id.record_id()
    );
    let redemption_id = ArtifactWriteRedemptionId::from_uuid(match &reserved.redemption.id.key {
        RecordIdKey::Uuid(value) => **value,
        other => panic!("unexpected redemption id key: {other:?}"),
    });
    assert!(
        store
            .finalize_artifact_write_capability(redemption_id, first_id,)
            .await
            .unwrap()
    );
    assert!(
        !store
            .finalize_artifact_write_capability(redemption_id, first_id)
            .await
            .unwrap()
    );
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                capability_id,
                &"b".repeat(64),
                &capability_task_id,
                "media:task:output:1",
                &"f".repeat(64),
                1,
                &["cui".into()],
                ArtifactId::new(),
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));

    let rebind_capability_id = ArtifactWriteCapabilityId::new();
    let rebind_task_id = TaskId::new().to_string();
    store
        .create_artifact_write_capability(ArtifactWriteCapabilityDraft {
            capability_id: rebind_capability_id,
            identity: identity.clone(),
            profile_key: "operator".into(),
            server_key: "optimization".into(),
            task_id: rebind_task_id.clone(),
            owner_kind: PrincipalKind::User,
            owner_issuer: "https://idp.example.com".into(),
            owner_subject: "alice-subject".into(),
            token_hash: "9".repeat(64),
            labels: vec!["cui".into()],
            max_artifact_count: 2,
            max_total_bytes: 6,
            expires_at: Utc::now() + TimeDelta::minutes(5),
        })
        .await
        .unwrap();
    let rebind_artifact_id = ArtifactId::new();
    let first_reservation = store
        .reserve_artifact_write_capability(
            rebind_capability_id,
            &"9".repeat(64),
            &rebind_task_id,
            "optimization:task:artifact:0",
            &"1".repeat(64),
            4,
            &["cui".into()],
            rebind_artifact_id,
        )
        .await
        .unwrap();
    let rebound = store
        .reserve_artifact_write_capability(
            rebind_capability_id,
            &"9".repeat(64),
            &rebind_task_id,
            "optimization:task:artifact:0",
            &"2".repeat(64),
            6,
            &["cui".into()],
            ArtifactId::new(),
        )
        .await
        .unwrap();
    assert!(rebound.request_matches);
    assert_eq!(rebound.redemption.id, first_reservation.redemption.id);
    assert_eq!(rebound.redemption.artifact, rebind_artifact_id.record_id());
    assert_eq!(rebound.redemption.request_hash, "2".repeat(64));
    assert_eq!(rebound.redemption.byte_len, 6);
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $capability;")
        .bind(("capability", rebind_capability_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let rebind_capability: ArtifactWriteCapabilityRecord =
        response.take::<Option<_>>(0).unwrap().unwrap();
    assert_eq!(rebind_capability.used_artifact_count, 1);
    assert_eq!(rebind_capability.used_total_bytes, 6);
    assert!(matches!(
        store
            .reserve_artifact_write_capability(
                rebind_capability_id,
                &"9".repeat(64),
                &rebind_task_id,
                "optimization:task:artifact:1",
                &"3".repeat(64),
                1,
                &["cui".into()],
                ArtifactId::new(),
            )
            .await,
        Err(StoreError::ArtifactWriteDenied)
    ));

    let link_id = ShareLinkId::new();
    store
        .create_artifact_share_link(ArtifactShareLinkDraft {
            link_id,
            artifact_id: first_id,
            identity,
            token_hash: "c".repeat(64),
            expires_at: Utc::now() + TimeDelta::minutes(5),
            max_downloads: Some(1),
        })
        .await
        .unwrap();
    assert!(
        store
            .redeem_public_share_link(&"c".repeat(64))
            .await
            .unwrap()
            .is_none(),
        "private artifacts must not redeem public links"
    );
    store
        .set_artifact_release_state(first_id, ArtifactReleaseState::Releasable)
        .await
        .unwrap();
    assert!(
        store
            .redeem_public_share_link(&"c".repeat(64))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .redeem_public_share_link(&"c".repeat(64))
            .await
            .unwrap()
            .is_none(),
        "share max_downloads must be atomic"
    );
}

#[tokio::test]
async fn recording_seal_publishes_artifact_bindings_and_outbox_atomically() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let database = format!("recording_test_{}", Uuid::now_v7().simple());
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &endpoint,
            "veoveo_integration",
            database,
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    let identity = store
        .ensure_identity(
            "tenant-recording",
            "recording-hub",
            "https://veoveo.local/services",
            "recording-hub",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let recording = store
        .create_recording(RecordingDraft {
            identity: identity.clone(),
            dataset: "world".into(),
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
        .open_segment(SegmentDraft {
            identity: identity.clone(),
            recording_id,
            segment_key: "2026-07-09/run-42.rrd".into(),
            ordinal: 0,
            relative_path: "world/2026-07-09/run-42.rrd".into(),
            start_time: Some(Utc::now()),
        })
        .await
        .unwrap();
    let second = store
        .open_segment(SegmentDraft {
            identity: identity.clone(),
            recording_id,
            segment_key: "2026-07-09/run-42.r1.rrd".into(),
            ordinal: 1,
            relative_path: "world/2026-07-09/run-42.r1.rrd".into(),
            start_time: Some(Utc::now()),
        })
        .await
        .unwrap();
    let first_id = SegmentId::from_uuid(record_uuid(&first.id));
    let second_id = SegmentId::from_uuid(record_uuid(&second.id));
    assert_eq!(
        store
            .freeze_segment(
                &identity,
                first_id,
                1_024,
                10,
                &"a".repeat(64),
                Some(Utc::now())
            )
            .await
            .unwrap()
            .state,
        SegmentState::Frozen
    );
    store
        .freeze_segment(
            &identity,
            second_id,
            2_048,
            20,
            &"b".repeat(64),
            Some(Utc::now()),
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .begin_recording_seal(&identity, recording_id, None)
            .await
            .unwrap()
            .state,
        RecordingState::Sealing
    );

    let first_artifact = ArtifactId::new();
    let second_artifact = ArtifactId::new();
    let manifest_artifact = ArtifactId::new();
    for (artifact_id, hash, filename) in [
        (first_artifact, "c".repeat(64), "run-42.rrd"),
        (second_artifact, "d".repeat(64), "run-42.r1.rrd"),
        (manifest_artifact, "e".repeat(64), "run-42.recording.json"),
    ] {
        store
            .create_artifact_occurrence(ArtifactOccurrenceDraft {
                artifact_id,
                identity: identity.clone(),
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
        .stage_segment_artifact(&identity, recording_id, first_id, first_artifact)
        .await
        .unwrap();
    store
        .stage_segment_artifact(&identity, recording_id, second_id, second_artifact)
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
            segments: vec![
                SegmentSealBinding {
                    segment_id: first_id,
                    artifact_id: first_artifact,
                },
                SegmentSealBinding {
                    segment_id: second_id,
                    artifact_id: second_artifact,
                },
            ],
            ended_at: Utc::now(),
        })
        .await
        .unwrap();
    assert_eq!(sealed.state, RecordingState::Sealed);
    assert_eq!(
        sealed.manifest_artifact,
        Some(manifest_artifact.record_id())
    );
    let segments = store
        .recording_segments(identity.tenant_id, recording_id, 10)
        .await
        .unwrap();
    assert!(
        segments
            .iter()
            .all(|segment| { segment.state == SegmentState::Sealed && segment.artifact.is_some() })
    );
    let outbox = store.read_outbox(0, 100).await.unwrap();
    assert!(outbox.events.iter().any(|event| {
        event.aggregate_id == recording_id.to_string() && event.event_type == "recording.sealed"
    }));

    let other = store
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
