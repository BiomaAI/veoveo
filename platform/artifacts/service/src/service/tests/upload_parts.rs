use super::native_database::{Database, context};
use super::upload_admission::{admission, install_profile};
use super::*;
use veoveo_platform_store as platform;

const PART_BYTES: i64 = 16 * 1024 * 1024;

fn claim(upload_id: uuid::Uuid, number: u32, bytes: i64) -> platform::ClaimArtifactUploadPart {
    platform::ClaimArtifactUploadPart {
        upload_id,
        part_number: number,
        byte_len: bytes,
        sha256: "a".repeat(64),
        lease_owner: uuid::Uuid::now_v7(),
        lease_seconds: 120,
        policy_digest: "b".repeat(64),
        quota_bytes: 1024 * 1024 * 1024,
        max_inflight_bytes: PART_BYTES,
        max_tenant_inflight_parts: 2,
    }
}

fn fence(
    request: &platform::ClaimArtifactUploadPart,
    part: &platform::ArtifactUploadPartRecord,
) -> platform::ArtifactUploadPartFence {
    platform::ArtifactUploadPartFence {
        upload_id: request.upload_id,
        part_number: request.part_number,
        lease_owner: request.lease_owner,
        generation: part.generation,
    }
}

fn id(record: &platform::RecordId) -> uuid::Uuid {
    match &record.key {
        platform::RecordIdKey::Uuid(value) => **value,
        _ => panic!("expected UUID"),
    }
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn upload_parts_fence_stale_attempts_preserve_receipts_and_bound_shared_memory() {
    let mut database = Database::start();
    let store = database.connect().await;
    let replica = database.connect().await;
    let alice = caller("alice", "acme", &[]);
    context(&store, &alice).await;
    install_profile(&store).await;
    let draft = admission(&store, &alice, PART_BYTES * 2 + 1).await;
    let upload_id = id(&draft.id);
    store
        .admit_artifact_upload(draft, 1024 * 1024 * 1024, 8)
        .await
        .unwrap();
    let first_request = claim(upload_id, 1, PART_BYTES);
    let first = store
        .claim_artifact_upload_part(first_request.clone())
        .await
        .unwrap();
    assert!(matches!(
        replica
            .claim_artifact_upload_part(claim(upload_id, 2, PART_BYTES))
            .await,
        Err(platform::StoreError::ArtifactUpload(
            platform::ArtifactUploadRejection::Busy
        ))
    ));
    store
        .release_artifact_upload_part(&fence(&first_request, &first))
        .await
        .unwrap();
    let retry_request = claim(upload_id, 1, PART_BYTES);
    let retry = replica
        .claim_artifact_upload_part(retry_request.clone())
        .await
        .unwrap();
    assert!(retry.generation > first.generation);
    assert!(
        store
            .accept_artifact_upload_part(&fence(&first_request, &first), "stale", &"b".repeat(64))
            .await
            .is_err()
    );
    replica
        .accept_artifact_upload_part(&fence(&retry_request, &retry), "part-1", &"b".repeat(64))
        .await
        .unwrap();
    let matching = store
        .claim_artifact_upload_part(claim(upload_id, 1, PART_BYTES))
        .await
        .unwrap();
    assert_eq!(matching.state, platform::ArtifactUploadPartState::Accepted);
    let mut conflicting = claim(upload_id, 1, PART_BYTES);
    conflicting.sha256 = "c".repeat(64);
    assert!(matches!(
        store.claim_artifact_upload_part(conflicting).await,
        Err(platform::StoreError::ArtifactUpload(
            platform::ArtifactUploadRejection::Conflict
        ))
    ));
    assert!(
        store
            .claim_artifact_upload_part(claim(upload_id, 2, 1))
            .await
            .is_err()
    );

    let second_request = claim(upload_id, 2, PART_BYTES);
    let second = store
        .claim_artifact_upload_part(second_request.clone())
        .await
        .unwrap();
    store
        .client()
        .query("UPDATE ONLY $part SET lease_until = $past;")
        .bind(("part", second.id.clone()))
        .bind(("past", Utc::now() - TimeDelta::seconds(1)))
        .await
        .unwrap()
        .check()
        .unwrap();
    let recovered_request = claim(upload_id, 2, PART_BYTES);
    let recovered = replica
        .claim_artifact_upload_part(recovered_request.clone())
        .await
        .unwrap();
    assert!(recovered.generation > second.generation);
    let receipt_fence = fence(&recovered_request, &recovered);
    replica
        .accept_artifact_upload_part(&receipt_fence, "part-2", &"b".repeat(64))
        .await
        .unwrap();
    replica
        .accept_artifact_upload_part(&receipt_fence, "part-2", &"b".repeat(64))
        .await
        .unwrap();
    let last_request = claim(upload_id, 3, 1);
    let last = store
        .claim_artifact_upload_part(last_request.clone())
        .await
        .unwrap();
    store
        .accept_artifact_upload_part(&fence(&last_request, &last), "part-3", &"b".repeat(64))
        .await
        .unwrap();
    let session = replica.artifact_upload(upload_id).await.unwrap().unwrap();
    assert_eq!(session.accepted_bytes, PART_BYTES * 2 + 1);
    assert_eq!(session.accepted_part_count, 3);
    let page = replica
        .artifact_upload_parts(upload_id, 1, 1, true)
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].part_number, 2);
    database.finish();
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn unknown_length_uploads_reserve_additional_windows_before_accepting_parts() {
    let mut database = Database::start();
    let store = database.connect().await;
    let alice = caller("alice", "acme", &[]);
    context(&store, &alice).await;
    install_profile(&store).await;
    let mut draft = admission(&store, &alice, PART_BYTES * 3).await;
    draft.descriptor.byte_len = None;
    draft.reserved_bytes = PART_BYTES;
    draft.layout.parallel_parts = 1;
    let upload_id = id(&draft.id);
    store
        .admit_artifact_upload(draft, 1024 * 1024 * 1024, 8)
        .await
        .unwrap();
    for number in 1..=2 {
        let request = claim(upload_id, number, PART_BYTES);
        let part = store
            .claim_artifact_upload_part(request.clone())
            .await
            .unwrap();
        let session = store.artifact_upload(upload_id).await.unwrap().unwrap();
        assert_eq!(session.reserved_bytes, PART_BYTES * i64::from(number));
        store
            .accept_artifact_upload_part(
                &fence(&request, &part),
                &format!("part-{number}"),
                &"b".repeat(64),
            )
            .await
            .unwrap();
    }
    let mut over_quota = claim(upload_id, 3, 7);
    over_quota.quota_bytes = PART_BYTES * 2;
    assert!(matches!(
        store.claim_artifact_upload_part(over_quota).await,
        Err(platform::StoreError::ArtifactUpload(
            platform::ArtifactUploadRejection::Quota
        ))
    ));
    assert!(
        store
            .claim_artifact_upload_part(claim(upload_id, 4, 1))
            .await
            .is_err()
    );
    assert_eq!(
        store
            .artifact_upload(upload_id)
            .await
            .unwrap()
            .unwrap()
            .reserved_bytes,
        PART_BYTES * 2
    );
    database.finish();
}
