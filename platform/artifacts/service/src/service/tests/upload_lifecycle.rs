//! Native metadata transactions; real S3 byte transfer is qualified separately.

use super::native_database::{Database, context};
use super::upload_admission::{admission, install_profile};
use super::upload_parts::{claim, fence as part_fence, id};
use super::*;
use veoveo_platform_store as platform;

const SIZE: i64 = 1024;

async fn work(
    store: &platform::PlatformStore,
    upload: uuid::Uuid,
) -> platform::ArtifactUploadFence {
    let owner = uuid::Uuid::now_v7();
    let row = store
        .claim_artifact_upload_work(upload, owner, 120)
        .await
        .unwrap();
    platform::ArtifactUploadFence {
        upload_id: upload,
        owner,
        generation: row.generation,
    }
}

async fn open(
    store: &platform::PlatformStore,
    actor: &PlaneCaller,
) -> platform::ArtifactUploadRecord {
    let draft = admission(store, actor, SIZE).await;
    let upload = id(&draft.id);
    store
        .admit_artifact_upload(draft, 1024 * 1024 * 1024, 8)
        .await
        .unwrap();
    let worker = work(store, upload).await;
    let row = store
        .initialize_artifact_upload(&worker, &format!("native-fixture-{upload}"))
        .await
        .unwrap();
    store.release_artifact_upload_work(&worker).await.unwrap();
    row
}

fn manifest() -> platform::ArtifactUploadManifest {
    platform::ArtifactUploadManifest {
        byte_len: SIZE,
        part_count: 1,
        sha256: None,
    }
}

async fn sealed(
    store: &platform::PlatformStore,
    actor: &PlaneCaller,
) -> platform::ArtifactUploadFence {
    let row = open(store, actor).await;
    let upload = id(&row.id);
    let request = claim(upload, 1, SIZE);
    let part = store
        .claim_artifact_upload_part(request.clone())
        .await
        .unwrap();
    store
        .accept_artifact_upload_part(&part_fence(&request, &part), "accepted", &row.policy_digest)
        .await
        .unwrap();
    store
        .freeze_artifact_upload(upload, manifest(), &row.policy_digest)
        .await
        .unwrap();
    let worker = work(store, upload).await;
    store.verify_artifact_upload(&worker).await.unwrap();
    worker
}

async fn usage(store: &platform::PlatformStore) -> platform::ArtifactStorageUsage {
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $usage;")
        .bind((
            "usage",
            platform::artifact_storage_usage_id(platform::deterministic_tenant_id("acme").unwrap()),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    response
        .take::<Option<platform::ArtifactStorageUsage>>(0)
        .unwrap()
        .unwrap()
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn upload_publication_commits_receipt_grants_audit_and_duplicate_cleanup_atomically() {
    let mut database = Database::start();
    let first = database.connect().await;
    let second = database.connect().await;
    let actor = caller("alice", "acme", &[]);
    context(&first, &actor).await;
    install_profile(&first).await;
    let left = sealed(&first, &actor).await;
    let right = sealed(&second, &actor).await;
    let sha = "a".repeat(64);
    let (a, b) = tokio::join!(
        first.publish_artifact_upload(&left, &sha, SIZE),
        second.publish_artifact_upload(&right, &sha, SIZE)
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.state, platform::ArtifactUploadState::Completed);
    assert_eq!(b.state, platform::ArtifactUploadState::Completed);
    assert_ne!(a.artifact, b.artifact);
    assert_ne!(a.cleanup_pending, b.cleanup_pending);
    let replay = second
        .publish_artifact_upload(&left, &sha, SIZE)
        .await
        .unwrap();
    assert_eq!(replay.artifact, a.artifact);
    assert_eq!(replay.completed_at, a.completed_at);
    assert!(first.cancel_artifact_upload(left.upload_id).await.is_err());
    assert!(
        first
            .publish_artifact_upload(&left, &"b".repeat(64), SIZE)
            .await
            .is_err()
    );
    let used = usage(&first).await;
    assert_eq!(
        (
            used.committed_bytes,
            used.reserved_bytes,
            used.cleanup_bytes,
            used.active_uploads
        ),
        (SIZE, 0, SIZE, 0)
    );
    for row in [&a, &b] {
        let aggregate = first
            .artifact_aggregate(platform::ArtifactId::from_uuid(id(&row.artifact)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(aggregate.blob.byte_len, SIZE);
        assert_eq!(aggregate.grants.len(), 1);
        assert_eq!(
            aggregate.grants[0].permission,
            platform::GrantPermission::Admin
        );
        assert_eq!(aggregate.occurrence.work_context, row.work_context);
        assert_eq!(
            aggregate.occurrence.policy_revision,
            row.authority.policy_revision
        );
        let mut response = first.client().query("SELECT * FROM audit_event WHERE action = 'artifact.upload.completed' AND resource_id = $artifact; SELECT * FROM outbox_event WHERE event_type = 'artifact.created' AND aggregate_id = $artifact;")
            .bind(("artifact", id(&row.artifact).to_string())).await.unwrap().check().unwrap();
        let audits: Vec<platform::AuditEventRecord> = response.take(0).unwrap();
        let events: Vec<platform::OutboxEventRecord> = response.take(1).unwrap();
        assert_eq!(audits.len(), 1);
        assert_eq!(events.len(), 1);
    }
    let (duplicate, retained) = if a.cleanup_pending {
        (&left, &right)
    } else {
        (&right, &left)
    };
    assert!(
        first
            .artifact_upload_cleanup_ready(duplicate)
            .await
            .unwrap()
    );
    assert!(!first.artifact_upload_cleanup_ready(retained).await.unwrap());
    assert!(
        first
            .finish_artifact_upload_cleanup(retained)
            .await
            .is_err()
    );
    first
        .finish_artifact_upload_cleanup(duplicate)
        .await
        .unwrap();
    first
        .finish_artifact_upload_cleanup(duplicate)
        .await
        .unwrap();
    assert_eq!(usage(&first).await.cleanup_bytes, 0);
    assert_eq!(usage(&first).await.committed_bytes, SIZE);
    database.finish();
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn cancellation_retains_cleanup_debt_and_waits_for_part_requests_to_stop() {
    let mut database = Database::start();
    let store = database.connect().await;
    let actor = caller("alice", "acme", &[]);
    context(&store, &actor).await;
    install_profile(&store).await;
    let row = open(&store, &actor).await;
    let upload = id(&row.id);
    let request = claim(upload, 1, SIZE);
    let part = store
        .claim_artifact_upload_part(request.clone())
        .await
        .unwrap();
    let cancelled = store.cancel_artifact_upload(upload).await.unwrap();
    assert_eq!(cancelled.state, platform::ArtifactUploadState::Cancelled);
    store.cancel_artifact_upload(upload).await.unwrap();
    let used = usage(&store).await;
    assert_eq!(
        (
            used.reserved_bytes,
            used.cleanup_bytes,
            used.active_uploads,
            used.inflight_bytes
        ),
        (0, SIZE, 0, SIZE)
    );
    let cleanup = work(&store, upload).await;
    assert!(!store.artifact_upload_cleanup_ready(&cleanup).await.unwrap());
    assert!(
        store
            .accept_artifact_upload_part(&part_fence(&request, &part), "late", &row.policy_digest)
            .await
            .is_err()
    );
    store
        .release_artifact_upload_part(&part_fence(&request, &part))
        .await
        .unwrap();
    assert!(store.artifact_upload_cleanup_ready(&cleanup).await.unwrap());
    store
        .finish_artifact_upload_cleanup(&cleanup)
        .await
        .unwrap();
    let used = usage(&store).await;
    assert_eq!(
        (used.cleanup_bytes, used.inflight_bytes, used.inflight_parts),
        (0, 0, 0)
    );
    database.finish();
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn sealed_upload_recovery_rejects_stale_workers_manifest_changes_and_revoked_authority() {
    let mut database = Database::start();
    let store = database.connect().await;
    let actor = caller("alice", "acme", &[]);
    let context = context(&store, &actor).await;
    install_profile(&store).await;
    let worker = sealed(&store, &actor).await;
    let row = store
        .artifact_upload(worker.upload_id)
        .await
        .unwrap()
        .unwrap();
    let mut changed = manifest();
    changed.byte_len += 1;
    assert!(
        store
            .freeze_artifact_upload(worker.upload_id, changed, &row.policy_digest)
            .await
            .is_err()
    );
    store
        .freeze_artifact_upload(worker.upload_id, manifest(), &row.policy_digest)
        .await
        .unwrap();
    assert!(
        store
            .claim_artifact_upload_part(claim(worker.upload_id, 1, SIZE))
            .await
            .is_err()
    );
    assert!(
        store
            .claim_artifact_upload_work(worker.upload_id, uuid::Uuid::now_v7(), 120)
            .await
            .is_err()
    );
    store
        .client()
        .query("UPDATE ONLY $upload SET lease_until = $past;")
        .bind(("upload", row.id.clone()))
        .bind(("past", Utc::now() - TimeDelta::seconds(1)))
        .await
        .unwrap()
        .check()
        .unwrap();
    let takeover = work(&store, worker.upload_id).await;
    assert!(takeover.generation > worker.generation);
    assert!(
        store
            .publish_artifact_upload(&worker, &"a".repeat(64), SIZE)
            .await
            .is_err()
    );
    assert!(
        store
            .renew_artifact_upload_work(&worker, 120)
            .await
            .is_err()
    );
    let candidates = store.artifact_upload_recovery_page(None, 1).await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].multipart_id, row.multipart_id);
    assert!(
        store
            .artifact_upload_recovery_page(Some(row.id.clone()), 1)
            .await
            .unwrap()
            .is_empty()
    );
    store
        .client()
        .query("UPDATE ONLY $context SET policy_revision = 'revoked';")
        .bind(("context", context))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        store
            .publish_artifact_upload(&takeover, &"a".repeat(64), SIZE)
            .await,
        Err(platform::StoreError::ArtifactUpload(
            platform::ArtifactUploadRejection::Denied
        ))
    ));
    assert!(
        store
            .artifact_aggregate(platform::ArtifactId::from_uuid(id(&row.artifact)))
            .await
            .unwrap()
            .is_none()
    );
    let failed = store
        .fail_artifact_upload(&takeover, platform::ArtifactUploadFailure::AuthorityChanged)
        .await
        .unwrap();
    assert_eq!(failed.state, platform::ArtifactUploadState::Failed);
    assert_eq!(usage(&store).await.committed_bytes, 0);
    assert_eq!(usage(&store).await.cleanup_bytes, SIZE);
    database.finish();
}
