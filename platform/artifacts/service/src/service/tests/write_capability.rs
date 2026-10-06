use super::*;
use std::num::NonZeroU32;

use super::native_store;

#[tokio::test]
async fn output_floor_survives_omitted_labels_and_changed_presentation_classification() {
    let (service, _) = service();
    let mut alice = caller(
        "alice",
        "acme",
        &["retained-home", "cui", "internal", "presentation"],
    );
    alice.identity.authority.output_policy.classification =
        Some(DataLabelId::parse("cui").unwrap());
    alice
        .identity
        .authority
        .output_policy
        .data_labels
        .insert(DataLabelId::parse("internal").unwrap());
    let original = alice.identity.authority.clone();
    let capability = service
        .issue_write_capability(
            &alice,
            IssueArtifactWriteCapabilityRequest {
                task_id: veoveo_artifact_contract::ArtifactTaskId::new(),
                expires_at: Utc::now() + TimeDelta::minutes(5),
                max_artifact_count: NonZeroU32::new(2).unwrap(),
                max_total_bytes: NonZeroU64::new(1024).unwrap(),
                required_data_labels: BTreeSet::from(
                    [DataLabelId::parse("retained-home").unwrap()],
                ),
            },
        )
        .await
        .unwrap();
    for (index, classification) in [None, Some(DataLabelId::parse("presentation").unwrap())]
        .into_iter()
        .enumerate()
    {
        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: capability.capability_id,
            task_id: capability.task_id,
            idempotency_key: ArtifactWriteIdempotencyKey::new(format!("output-{index}")).unwrap(),
            artifact: PutArtifactRequest {
                classification,
                ..Default::default()
            },
        };
        let artifact = service
            .redeem_write_capability(
                capability.secret.expose_secret(),
                request.clone(),
                b"bounded output".to_vec(),
            )
            .await
            .unwrap();
        for label in ["retained-home", "cui", "internal"] {
            assert!(
                artifact
                    .compliance
                    .data_labels
                    .contains(&DataLabelId::parse(label).unwrap())
            );
        }
        let retry = service
            .redeem_write_capability(
                capability.secret.expose_secret(),
                request,
                b"bounded output".to_vec(),
            )
            .await
            .unwrap();
        assert_eq!(retry.artifact_id(), artifact.artifact_id());
        assert_eq!(
            artifact.compliance.owner,
            Some(original.output_policy.owner.clone())
        );
        assert!(
            service
                .download(
                    &caller("alice", "acme", &[]),
                    artifact.artifact_id(),
                    None,
                    DownloadBody::Include
                )
                .await
                .is_err()
        );
    }
    assert_eq!(alice.identity.authority, original);
}

#[tokio::test]
async fn inherited_output_labels_persist_across_independent_native_service_instances() {
    let db = native_store::TestDb::new().await;
    let alice = caller("alice", "acme", &["retained-home"]);
    super::native_database::context(&db.a, &alice).await;
    let blobs = InMemoryBlobStore::default();
    let first = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(db.a.clone()),
        blobs.clone(),
        "http://fixture",
        1024,
    );
    let capability = first
        .issue_write_capability(
            &alice,
            IssueArtifactWriteCapabilityRequest {
                task_id: veoveo_artifact_contract::ArtifactTaskId::new(),
                expires_at: Utc::now() + TimeDelta::minutes(5),
                max_artifact_count: NonZeroU32::new(1).unwrap(),
                max_total_bytes: NonZeroU64::new(1024).unwrap(),
                required_data_labels: BTreeSet::from(
                    [DataLabelId::parse("retained-home").unwrap()],
                ),
            },
        )
        .await
        .unwrap();
    drop(first);
    let second = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(db.b.clone()),
        blobs,
        "http://fixture",
        1024,
    );
    let request = RedeemArtifactWriteCapabilityRequest {
        capability_id: capability.capability_id,
        task_id: capability.task_id,
        idempotency_key: ArtifactWriteIdempotencyKey::new("stdout").unwrap(),
        artifact: PutArtifactRequest::default(),
    };
    let artifact = second
        .redeem_write_capability(
            capability.secret.expose_secret(),
            request.clone(),
            b"output".to_vec(),
        )
        .await
        .unwrap();
    assert!(
        artifact
            .compliance
            .data_labels
            .contains(&DataLabelId::parse("retained-home").unwrap())
    );
    let retry = second
        .redeem_write_capability(
            capability.secret.expose_secret(),
            request.clone(),
            b"output".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(artifact.artifact_id(), retry.artifact_id());
    let row = veoveo_platform_store::ArtifactWriteCapabilityId::from_uuid(
        capability.capability_id.as_uuid(),
    )
    .record_id();
    let mut saved =
        db.a.client()
            .query(include_str!("../../../tests/queries/service/tests/write_capability/inherited_output_labels_persist_across_independent_native_service_instances.surql"))
            .bind(("capability", row.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
    let saved: veoveo_platform_store::ArtifactWriteCapabilityRecord =
        saved.take::<Option<_>>(0).unwrap().unwrap();
    assert_eq!(
        saved.authority.data_labels,
        vec!["retained-home".to_owned()]
    );
    assert_eq!(saved.task_id, capability.task_id.to_string());
    // Current writers persist canonical text. Old alias/corrupt retained bindings
    // require the declared installation drain; reads must not repair them.
    for stale in [
        capability.task_id.as_uuid().simple().to_string(),
        "not-a-task".to_owned(),
    ] {
        let mut changed = db
            .a
            .client()
            .query(include_str!(
                "../../../tests/queries/service/tests/write_capability/mutate_retained_task.surql"
            ))
            .bind(("capability", row.clone()))
            .bind(("task_id", stale.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let before: veoveo_platform_store::ArtifactWriteCapabilityRecord =
            changed.take::<Option<_>>(0).unwrap().unwrap();
        assert!(
            second
                .redeem_write_capability(
                    capability.secret.expose_secret(),
                    request.clone(),
                    b"output".to_vec()
                )
                .await
                .is_err()
        );
        let mut retained = db.a.client().query(include_str!("../../../tests/queries/service/tests/write_capability/inherited_output_labels_persist_across_independent_native_service_instances.surql"))
            .bind(("capability",row.clone())).await.unwrap().check().unwrap();
        let after: veoveo_platform_store::ArtifactWriteCapabilityRecord =
            retained.take::<Option<_>>(0).unwrap().unwrap();
        assert_eq!(after, before);
        assert_eq!(after.task_id, stale);
    }
}

#[tokio::test]
async fn output_floor_requires_caller_clearance_at_issuance() {
    let (service, _) = service();
    let alice = caller("alice", "acme", &[]);
    let mut request = IssueArtifactWriteCapabilityRequest {
        task_id: veoveo_artifact_contract::ArtifactTaskId::new(),
        expires_at: Utc::now() + TimeDelta::minutes(5),
        max_artifact_count: NonZeroU32::new(1).unwrap(),
        max_total_bytes: NonZeroU64::new(1024).unwrap(),
        required_data_labels: BTreeSet::from([DataLabelId::parse("unheld-label").unwrap()]),
    };
    assert!(matches!(
        service
            .issue_write_capability(&alice, request.clone())
            .await,
        Err(ArtifactPlaneError::InvalidRequest(_))
    ));
    request.required_data_labels.clear();
    let mut invalid_policy = alice.clone();
    invalid_policy
        .identity
        .authority
        .output_policy
        .classification = Some(DataLabelId::parse("unheld-label").unwrap());
    assert!(matches!(
        service
            .issue_write_capability(&invalid_policy, request)
            .await,
        Err(ArtifactPlaneError::InvalidRequest(_))
    ));
}

#[tokio::test]
async fn first_successful_reservation_readback_rejects_corrupt_bindings_before_occurrence_effects()
{
    let db = native_store::TestDb::new().await;
    let alice = caller("alice", "acme", &[]);
    super::native_database::context(&db.a, &alice).await;
    let service = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(db.a.clone()),
        InMemoryBlobStore::default(),
        "http://fixture",
        1024,
    );
    let task_id =
        veoveo_artifact_contract::ArtifactTaskId::parse("01900000-0000-7000-8000-000000000001")
            .unwrap();
    for (event, expected_capability, expected_redemption, foreign_task) in [
        (
            include_str!(
                "../../../tests/queries/service/tests/write_capability/corrupt_first_redemption_readback.surql"
            ),
            task_id.to_string(),
            "not-a-task".to_owned(),
            false,
        ),
        (
            include_str!(
                "../../../tests/queries/service/tests/write_capability/corrupt_first_capability_readback.surql"
            ),
            task_id.as_uuid().simple().to_string(),
            task_id.to_string(),
            false,
        ),
        (
            include_str!(
                "../../../tests/queries/service/tests/write_capability/corrupt_first_native_task_readback.surql"
            ),
            task_id.to_string(),
            task_id.to_string(),
            true,
        ),
    ] {
        let capability = service
            .issue_write_capability(
                &alice,
                IssueArtifactWriteCapabilityRequest {
                    task_id,
                    expires_at: Utc::now() + TimeDelta::minutes(5),
                    max_artifact_count: NonZeroU32::new(1).unwrap(),
                    max_total_bytes: NonZeroU64::new(1024).unwrap(),
                    required_data_labels: BTreeSet::new(),
                },
            )
            .await
            .unwrap();
        db.a.client().query(event).await.unwrap().check().unwrap();
        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: capability.capability_id,
            task_id,
            idempotency_key: ArtifactWriteIdempotencyKey::new("first-readback").unwrap(),
            artifact: PutArtifactRequest::default(),
        };
        assert!(
            service
                .redeem_write_capability(
                    capability.secret.expose_secret(),
                    request.clone(),
                    b"output".to_vec()
                )
                .await
                .is_err()
        );
        let row = veoveo_platform_store::ArtifactWriteCapabilityId::from_uuid(
            capability.capability_id.as_uuid(),
        )
        .record_id();
        let mut snapshot = db
            .a
            .client()
            .query(include_str!(
                "../../../tests/queries/service/tests/write_capability/read_binding_snapshot.surql"
            ))
            .bind(("capability", row.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let before_capability: veoveo_platform_store::ArtifactWriteCapabilityRecord =
            snapshot.take::<Option<_>>(0).unwrap().unwrap();
        let before_redemptions: Vec<veoveo_platform_store::ArtifactWriteRedemptionRecord> =
            snapshot.take(1).unwrap();
        let before_occurrences: Vec<veoveo_platform_store::RecordId> = snapshot.take(2).unwrap();
        assert_eq!(before_capability.task_id, expected_capability);
        assert_eq!(before_capability.used_artifact_count, 1);
        assert_eq!(before_capability.used_total_bytes, 6);
        assert_eq!(before_redemptions.len(), 1);
        assert_eq!(before_redemptions[0].task_id, expected_redemption);
        let expected_task = if foreign_task {
            veoveo_platform_store::RecordId::new("task", "foreign")
        } else {
            veoveo_platform_store::task_record_id(veoveo_types::TaskId::from_uuid(
                task_id.as_uuid(),
            ))
        };
        assert_eq!(before_redemptions[0].task, expected_task);
        assert_eq!(
            before_redemptions[0].state,
            veoveo_platform_store::ArtifactWriteRedemptionState::Reserved
        );
        assert!(before_occurrences.is_empty());
        assert!(
            service
                .redeem_write_capability(
                    capability.secret.expose_secret(),
                    request,
                    b"output".to_vec()
                )
                .await
                .is_err()
        );
        let mut snapshot = db
            .a
            .client()
            .query(include_str!(
                "../../../tests/queries/service/tests/write_capability/read_binding_snapshot.surql"
            ))
            .bind(("capability", row))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            snapshot
                .take::<Option<veoveo_platform_store::ArtifactWriteCapabilityRecord>>(0)
                .unwrap()
                .unwrap(),
            before_capability
        );
        assert_eq!(
            snapshot
                .take::<Vec<veoveo_platform_store::ArtifactWriteRedemptionRecord>>(1)
                .unwrap(),
            before_redemptions
        );
        assert_eq!(
            snapshot
                .take::<Vec<veoveo_platform_store::RecordId>>(2)
                .unwrap(),
            before_occurrences
        );
    }
}

#[tokio::test]
async fn successful_rebind_readback_rejects_changed_capability_binding() {
    let db = native_store::TestDb::new().await;
    let alice = caller("alice", "acme", &[]);
    super::native_database::context(&db.a, &alice).await;
    let service = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(db.a.clone()),
        InMemoryBlobStore::default(),
        "http://fixture",
        1024,
    );
    let task_id =
        veoveo_artifact_contract::ArtifactTaskId::parse("01900000-0000-7000-8000-000000000001")
            .unwrap();
    let capability = service
        .issue_write_capability(
            &alice,
            IssueArtifactWriteCapabilityRequest {
                task_id,
                expires_at: Utc::now() + TimeDelta::minutes(5),
                max_artifact_count: NonZeroU32::new(1).unwrap(),
                max_total_bytes: NonZeroU64::new(1024).unwrap(),
                required_data_labels: BTreeSet::new(),
            },
        )
        .await
        .unwrap();
    let id = veoveo_platform_store::ArtifactWriteCapabilityId::from_uuid(
        capability.capability_id.as_uuid(),
    );
    let token_hash = secret_hash(
        b"veoveo.artifact-write.v1",
        capability.secret.expose_secret(),
    );
    let reserved =
        db.a.reserve_artifact_write_capability(
            id,
            &token_hash,
            &task_id,
            "rebind-readback",
            &"a".repeat(64),
            6,
            &[],
            veoveo_platform_store::ArtifactId::new(),
        )
        .await
        .unwrap();
    db.a.client().query(include_str!("../../../tests/queries/service/tests/write_capability/corrupt_rebind_capability_readback.surql")).await.unwrap().check().unwrap();
    assert!(
        db.b.reserve_artifact_write_capability(
            id,
            &token_hash,
            &task_id,
            "rebind-readback",
            &"b".repeat(64),
            6,
            &[],
            veoveo_platform_store::ArtifactId::new()
        )
        .await
        .is_err()
    );
    let mut snapshot =
        db.a.client()
            .query(include_str!(
                "../../../tests/queries/service/tests/write_capability/read_binding_snapshot.surql"
            ))
            .bind(("capability", id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
    let before: veoveo_platform_store::ArtifactWriteCapabilityRecord =
        snapshot.take::<Option<_>>(0).unwrap().unwrap();
    let redemptions: Vec<veoveo_platform_store::ArtifactWriteRedemptionRecord> =
        snapshot.take(1).unwrap();
    assert!(
        snapshot
            .take::<Vec<veoveo_platform_store::RecordId>>(2)
            .unwrap()
            .is_empty()
    );
    assert_eq!(before.task_id, task_id.as_uuid().simple().to_string());
    assert_eq!(before.used_artifact_count, 1);
    assert_eq!(before.used_total_bytes, 6);
    assert_eq!(redemptions.len(), 1);
    assert_eq!(redemptions[0].id, reserved.redemption.id);
    assert_eq!(redemptions[0].artifact, reserved.redemption.artifact);
    assert_eq!(redemptions[0].task, reserved.redemption.task);
    assert_eq!(redemptions[0].task_id, reserved.redemption.task_id);
    assert_eq!(redemptions[0].request_hash, "b".repeat(64));
    assert_eq!(
        redemptions[0].state,
        veoveo_platform_store::ArtifactWriteRedemptionState::Reserved
    );
    assert!(
        db.b.reserve_artifact_write_capability(
            id,
            &token_hash,
            &task_id,
            "rebind-readback",
            &"b".repeat(64),
            6,
            &[],
            veoveo_platform_store::ArtifactId::new()
        )
        .await
        .is_err()
    );
    let mut snapshot =
        db.a.client()
            .query(include_str!(
                "../../../tests/queries/service/tests/write_capability/read_binding_snapshot.surql"
            ))
            .bind(("capability", id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
    assert_eq!(
        snapshot
            .take::<Option<veoveo_platform_store::ArtifactWriteCapabilityRecord>>(0)
            .unwrap()
            .unwrap(),
        before
    );
    assert_eq!(
        snapshot
            .take::<Vec<veoveo_platform_store::ArtifactWriteRedemptionRecord>>(1)
            .unwrap(),
        redemptions
    );
    assert!(
        snapshot
            .take::<Vec<veoveo_platform_store::RecordId>>(2)
            .unwrap()
            .is_empty()
    );
}
