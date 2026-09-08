//! Real database races protect upload objects from every shared artifact writer.

use super::native_database::Database;
use super::*;

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns an isolated SurrealDB 3.2.4 process"]
async fn immutable_blob_registration_survives_concurrent_writers_and_failed_publication() {
    let mut database = Database::start();
    let first_store = database.connect().await;
    let second_store = database.connect().await;
    let blobs = InMemoryBlobStore::default();
    let first = ArtifactService::new(
        crate::SurrealArtifactRepository::new(first_store),
        blobs.clone(),
    );
    let second = ArtifactService::new(
        crate::SurrealArtifactRepository::new(second_store),
        blobs.clone(),
    );
    let alice = caller("alice", "acme", &[]);
    let bytes = b"identical artifact bytes".to_vec();
    let sha = compute_sha(&bytes);
    let publications =
        (0..8).map(|index| {
            let service = if index % 2 == 0 { &first } else { &second };
            let alice = &alice;
            let sha = sha.clone();
            let bytes = bytes.clone();
            async move {
                let key = format!("tenants/acme/uploads/{index}");
                service.store.put(&key, bytes.clone()).await.unwrap();
                service.record_occurrence(ArtifactId::new(), ArtifactService::<
                crate::SurrealArtifactRepository, InMemoryBlobStore>::actor(alice).unwrap(),
                alice.identity.authority.clone(), BTreeSet::new(), PutArtifactRequest::default(),
                sha, bytes.len() as u64, key).await.unwrap()
            }
        });
    let results = futures::future::join_all(publications).await;
    let retained_key = results[0].object_key.clone();
    assert!(retained_key.starts_with("tenants/acme/uploads/"));
    let ids: BTreeSet<_> = results
        .iter()
        .map(|result| result.metadata.artifact_id)
        .collect();
    assert_eq!(ids.len(), 8);
    for result in &results {
        assert_eq!(result.object_key, retained_key);
        assert_eq!(result.metadata.byte_len, bytes.len() as u64);
        let downloaded = second
            .get(&alice, &result.metadata.artifact_id, AccessLevel::Read)
            .await
            .unwrap();
        assert_eq!(downloaded.bytes, bytes);
    }

    // The ordinary writer must also reuse an earlier upload's opaque mapping.
    let ordinary = second
        .put(&alice, PutArtifactRequest::default(), bytes.clone())
        .await
        .unwrap();
    let ordinary = first
        .repository
        .get_artifact(ordinary.artifact_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ordinary.object_key, retained_key);

    // Equal digest with different content metadata aborts the entire publication.
    let failed_id = ArtifactId::new();
    let failed = second
        .record_occurrence(
            failed_id,
            ArtifactService::<crate::SurrealArtifactRepository, InMemoryBlobStore>::actor(&alice)
                .unwrap(),
            alice.identity.authority.clone(),
            BTreeSet::new(),
            PutArtifactRequest::default(),
            sha,
            bytes.len() as u64 + 1,
            "unpublished-conflict".into(),
        )
        .await;
    assert!(
        matches!(failed, Err(ArtifactPlaneError::Conflict(_))),
        "{failed:?}"
    );
    assert!(
        first
            .repository
            .get_artifact(failed_id)
            .await
            .unwrap()
            .is_none()
    );
    for result in results {
        let stored = first
            .repository
            .get_artifact(result.metadata.artifact_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.object_key, retained_key);
        assert_eq!(stored.metadata.byte_len, bytes.len() as u64);
    }
    database.finish();
}
