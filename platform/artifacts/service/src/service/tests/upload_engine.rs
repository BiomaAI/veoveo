//! Full service transfer and recovery against native Store and a shared byte backend.

use super::native_database::{Database, context};
use super::upload_admission::install_profile_policy;
use super::*;
use crate::{ArtifactObjectStore, uploads::UploadService};
use axum::body::Bytes;
use std::{num::NonZeroU32, sync::Arc, time::Duration};
use veoveo_mcp_contract as contract;
use veoveo_platform_store as platform;

async fn fixture(
    store: &platform::PlatformStore,
    actor: &PlaneCaller,
) -> contract::VerifiedArtifactUploadIdentity {
    context(store, actor).await;
    let policy = serde_json::json!({
        "max_object_bytes": 107374182400_u64, "tenant_quota_bytes": 214748364800_u64,
        "max_active_uploads_per_tenant": 8, "part_bytes": 16777216, "max_part_bytes": 67108864,
        "max_parts": 10000, "parallel_parts": 4, "max_inflight_bytes": 134217728,
        "inactivity_seconds": 86400, "lifetime_seconds": 604800, "part_timeout_seconds": 120,
        "allowed_mime_types": ["application/octet-stream"]
    });
    install_profile_policy(store, Some(serde_json::from_value(policy).unwrap())).await;
    let version = store
        .artifact_upload_authority_version(
            "acme",
            actor.identity.authority.work_context.as_str(),
            "fixture",
        )
        .await
        .unwrap()
        .unwrap();
    let mut identity = actor.identity.clone();
    identity.server = contract::ServerSlug::new(contract::ARTIFACT_UPLOAD_AUDIENCE).unwrap();
    identity.profile = contract::GatewayProfileId::new("fixture").unwrap();
    identity
        .actor
        .scopes
        .insert(contract::ScopeName::new("artifact:upload").unwrap());
    contract::VerifiedArtifactUploadIdentity {
        identity,
        authorization: contract::ArtifactUploadAuthority {
            control_plane_sha256: contract::UploadSha256::parse(version.control_plane_sha256)
                .unwrap(),
            context_digest: contract::UploadSha256::parse(version.context_digest).unwrap(),
        },
    }
}

fn stream(bytes: Vec<u8>) -> BlobStream {
    let chunks: Vec<_> = bytes
        .chunks(8192)
        .map(|bytes| Ok(Bytes::copy_from_slice(bytes)))
        .collect();
    Box::pin(futures::stream::iter(chunks))
}

fn descriptor(bytes: usize) -> contract::CreateArtifactUpload {
    contract::CreateArtifactUpload {
        filename: "measurements.bin".into(),
        mime_type: "application/octet-stream".into(),
        byte_len: Some(bytes as u64),
        sha256: None,
    }
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn upload_service_replays_admission_and_parts_then_recovers_completion_on_another_replica() {
    let mut database = Database::start();
    let store = database.connect().await;
    let actor = caller("alice", "acme", &[]);
    let verified = fixture(&store, &actor).await;
    let objects =
        ArtifactObjectStore::with_multipart(Arc::new(object_store::memory::InMemory::new()));
    let service = UploadService::new(store.clone(), objects.clone());
    let policy = service.policy(&verified).await.unwrap();
    assert!(policy.allowed);
    assert_eq!(policy.available_bytes, Some(214748364800));
    let bytes: Vec<_> = (0..262144).map(|i| (i % 251) as u8).collect();
    let sha = contract::UploadSha256::parse(hex::encode(Sha256::digest(&bytes))).unwrap();
    let key = contract::ArtifactUploadRequestId::new();
    let (session, created) = service
        .create(&verified, key, descriptor(bytes.len()))
        .await
        .unwrap();
    assert!(created);
    let (replay, created) = service
        .create(&verified, key, descriptor(bytes.len()))
        .await
        .unwrap();
    assert!(!created);
    assert_eq!(session.upload_id, replay.upload_id);
    let mut foreign = verified.clone();
    foreign.identity.actor.id = contract::PrincipalId::new("another-person").unwrap();
    assert!(matches!(
        service.status(&foreign, session.upload_id, 0).await,
        Err(crate::uploads::UploadFault(
            contract::UploadErrorCode::NotFound
        ))
    ));
    let part = service
        .put_part(
            &verified,
            session.upload_id,
            NonZeroU32::new(1).unwrap(),
            bytes.len() as u64,
            sha.clone(),
            stream(bytes.clone()),
        )
        .await
        .unwrap();
    let repeated = service
        .put_part(
            &verified,
            session.upload_id,
            NonZeroU32::new(1).unwrap(),
            bytes.len() as u64,
            sha.clone(),
            stream(vec![]),
        )
        .await
        .unwrap();
    assert_eq!(part, repeated);
    let complete = contract::CompleteArtifactUpload {
        byte_len: bytes.len() as u64,
        part_count: NonZeroU32::new(1).unwrap(),
        sha256: Some(sha.clone()),
    };
    let finalizing = service
        .complete(&verified, session.upload_id, complete.clone())
        .await
        .unwrap();
    assert_eq!(finalizing.state, contract::ArtifactUploadState::Finalizing);
    assert_eq!(finalizing.accepted_bytes, bytes.len() as u64);
    assert!(finalizing.receipt.is_none());
    drop(service);
    let replica = UploadService::new(database.connect().await, objects.clone());
    let recovery = tokio::spawn(replica.clone().run_recovery());
    let completed = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let row = replica
                .status(&verified, session.upload_id, 0)
                .await
                .unwrap();
            if row.state == contract::ArtifactUploadState::Completed {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("durable completion did not recover");
    let receipt = completed.receipt.unwrap();
    assert_eq!(receipt.sha256, sha);
    assert_eq!(receipt.byte_len, bytes.len() as u64);
    let again = replica
        .complete(&verified, session.upload_id, complete)
        .await
        .unwrap()
        .receipt
        .unwrap();
    assert_eq!(receipt, again);
    let ordinary = ArtifactService::new(
        crate::SurrealArtifactRepository::new(store.clone()),
        objects,
    );
    let downloaded = ordinary
        .get(&actor, &receipt.artifact_id, AccessLevel::Read)
        .await
        .unwrap();
    assert_eq!(downloaded.bytes, bytes);
    let serialized = serde_json::to_string(&replay).unwrap();
    assert!(!serialized.contains("multipart"));
    assert!(!serialized.contains("object_key"));
    recovery.abort();
    let _ = recovery.await;
    database.finish();
}

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn dropped_upload_request_releases_memory_and_cancellation_recovers_physical_cleanup() {
    let mut database = Database::start();
    let store = database.connect().await;
    let actor = caller("alice", "acme", &[]);
    let verified = fixture(&store, &actor).await;
    let objects =
        ArtifactObjectStore::with_multipart(Arc::new(object_store::memory::InMemory::new()));
    let service = UploadService::new(store.clone(), objects.clone());
    let (session, _) = service
        .create(
            &verified,
            contract::ArtifactUploadRequestId::new(),
            descriptor(1024),
        )
        .await
        .unwrap();
    let copy = service.clone();
    let identity = verified.clone();
    let request = tokio::spawn(async move {
        copy.put_part(
            &identity,
            session.upload_id,
            NonZeroU32::new(1).unwrap(),
            1024,
            contract::UploadSha256::parse("a".repeat(64)).unwrap(),
            Box::pin(futures::stream::pending()),
        )
        .await
    });
    let tenant = platform::deterministic_tenant_id("acme").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while store
            .artifact_upload_storage_usage(tenant)
            .await
            .unwrap()
            .unwrap()
            .inflight_bytes
            == 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    request.abort();
    let _ = request.await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while store
            .artifact_upload_storage_usage(tenant)
            .await
            .unwrap()
            .unwrap()
            .inflight_bytes
            != 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("dropped request retained its transfer budget");
    service.cancel(&verified, session.upload_id).await.unwrap();
    let recovery = tokio::spawn(service.clone().run_recovery());
    tokio::time::timeout(Duration::from_secs(20), async {
        while store
            .artifact_upload(session.upload_id.as_uuid())
            .await
            .unwrap()
            .unwrap()
            .cleanup_pending
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("cancelled upload cleanup was not recovered");
    let usage = store
        .artifact_upload_storage_usage(tenant)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            usage.reserved_bytes,
            usage.cleanup_bytes,
            usage.inflight_bytes,
            usage.active_uploads
        ),
        (0, 0, 0, 0)
    );
    let mut stale = verified.clone();
    stale.authorization.control_plane_sha256 =
        contract::UploadSha256::parse("d".repeat(64)).unwrap();
    assert!(matches!(
        service
            .create(
                &stale,
                contract::ArtifactUploadRequestId::new(),
                descriptor(1)
            )
            .await,
        Err(crate::uploads::UploadFault(
            contract::UploadErrorCode::Denied
        ))
    ));
    recovery.abort();
    let _ = recovery.await;
    database.finish();
}
