use super::*;
use std::num::NonZeroU32;

#[path = "../../../../../../testing/fixtures/store.rs"]
mod native_store;

#[tokio::test]
async fn output_floor_survives_omitted_labels_and_changed_presentation_classification() {
    let (service, _) = service();
    let mut alice = caller(
        "alice",
        "acme",
        &["retained-home", "cui", "internal", "presentation"],
    );
    alice.identity.authority.output_policy.classification = Some(DataLabelId::new("cui").unwrap());
    alice
        .identity
        .authority
        .output_policy
        .data_labels
        .insert(DataLabelId::new("internal").unwrap());
    let original = alice.identity.authority.clone();
    let capability = service
        .issue_write_capability(
            &alice,
            IssueArtifactWriteCapabilityRequest {
                task_id: uuid::Uuid::now_v7().to_string(),
                expires_at: Utc::now() + TimeDelta::minutes(5),
                max_artifact_count: NonZeroU32::new(2).unwrap(),
                max_total_bytes: NonZeroU64::new(1024).unwrap(),
                required_data_labels: BTreeSet::from([DataLabelId::new("retained-home").unwrap()]),
            },
        )
        .await
        .unwrap();
    for (index, classification) in [None, Some(DataLabelId::new("presentation").unwrap())]
        .into_iter()
        .enumerate()
    {
        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: capability.capability_id,
            task_id: capability.task_id.clone(),
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
                    .contains(&DataLabelId::new(label).unwrap())
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
        assert_eq!(retry.artifact_id, artifact.artifact_id);
        assert_eq!(
            artifact.compliance.owner,
            Some(original.output_policy.owner.clone())
        );
        assert!(
            service
                .download(
                    &caller("alice", "acme", &[]),
                    artifact.artifact_id,
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
                task_id: uuid::Uuid::now_v7().to_string(),
                expires_at: Utc::now() + TimeDelta::minutes(5),
                max_artifact_count: NonZeroU32::new(1).unwrap(),
                max_total_bytes: NonZeroU64::new(1024).unwrap(),
                required_data_labels: BTreeSet::from([DataLabelId::new("retained-home").unwrap()]),
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
        task_id: capability.task_id.clone(),
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
            .contains(&DataLabelId::new("retained-home").unwrap())
    );
    let retry = second
        .redeem_write_capability(
            capability.secret.expose_secret(),
            request,
            b"output".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(artifact.artifact_id, retry.artifact_id);
    let row = veoveo_platform_store::ArtifactWriteCapabilityId::from_uuid(
        capability.capability_id.as_uuid(),
    )
    .record_id();
    let mut saved =
        db.a.client()
            .query("SELECT * FROM ONLY $capability;")
            .bind(("capability", row))
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
}

#[tokio::test]
async fn output_floor_requires_caller_clearance_at_issuance() {
    let (service, _) = service();
    let alice = caller("alice", "acme", &[]);
    let mut request = IssueArtifactWriteCapabilityRequest {
        task_id: uuid::Uuid::now_v7().to_string(),
        expires_at: Utc::now() + TimeDelta::minutes(5),
        max_artifact_count: NonZeroU32::new(1).unwrap(),
        max_total_bytes: NonZeroU64::new(1024).unwrap(),
        required_data_labels: BTreeSet::from([DataLabelId::new("unheld-label").unwrap()]),
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
        .classification = Some(DataLabelId::new("unheld-label").unwrap());
    assert!(matches!(
        service
            .issue_write_capability(&invalid_policy, request)
            .await,
        Err(ArtifactPlaneError::InvalidRequest(_))
    ));
}
