//! A real SurrealDB process, independent service instances, and atomic read quotas.
use super::super::native_database::{Database, context};
use super::*;
use veoveo_platform_store as platform;

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns and removes an isolated in-memory SurrealDB 3.2.4 process"]
async fn artifact_read_delegation_survives_service_recreation_and_enforces_native_atomic_state() {
    let mut database = Database::start();
    let store = database.connect().await;
    let mut alice = caller("alice", "acme", &["recordings"]);
    alice
        .memberships
        .insert(veoveo_mcp_contract::GroupMembership {
            group: veoveo_mcp_contract::GroupId::new("readers").unwrap(),
            role: veoveo_mcp_contract::GroupRole::Read,
        });
    let context_id = context(&store, &alice).await;
    let blobs = InMemoryBlobStore::default();
    let service = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(store.clone()),
        blobs.clone(),
        "http://fixture",
        1024,
    );
    let first = service
        .put(&alice, PutArtifactRequest::default(), vec![1; 3])
        .await
        .unwrap()
        .artifact_id;
    let second = service
        .put(&alice, PutArtifactRequest::default(), vec![2; 3])
        .await
        .unwrap()
        .artifact_id;
    let cap = service
        .issue_read_capability(&alice, request(2, 5))
        .await
        .unwrap();
    drop(service);
    let other_store = database.connect().await;
    let one = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(store.clone()),
        blobs.clone(),
        "http://fixture",
        1024,
    );
    let two = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(other_store),
        blobs,
        "http://fixture",
        1024,
    );
    let (first_result, second_result) =
        tokio::join!(read(&one, &cap, first), read(&two, &cap, second));
    assert_eq!(
        usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
        1,
        "native quota admitted two competing reads or neither: {first_result:?}, {second_result:?}"
    );
    let winner = if first_result.is_ok() { first } else { second };
    read(&two, &cap, winner).await.unwrap();
    let record =
        platform::ArtifactReadCapabilityId::from_uuid(cap.capability_id.as_uuid()).record_id();
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record;")
        .bind(("record", record.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let persisted: platform::ArtifactReadCapabilityRecord =
        response.take::<Option<_>>(0).unwrap().unwrap();
    assert_eq!(persisted.used_total_bytes, 3);
    assert_eq!(persisted.admitted_artifacts.len(), 1);
    assert_eq!(persisted.memberships.len(), 1);
    assert_eq!(persisted.memberships[0].group_key, "readers");
    assert_eq!(persisted.labels, ["recordings"]);
    assert_ne!(persisted.token_hash, cap.secret.expose_secret());
    store
        .client()
        .query("UPDATE $record SET expires_at = $past;")
        .bind(("record", record))
        .bind(("past", Utc::now() - TimeDelta::seconds(1)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        matches!(
            read(&one, &cap, winner).await,
            Err(ArtifactPlaneError::Unauthenticated)
        ),
        "expired admission was replayed"
    );

    let cap = one
        .issue_read_capability(&alice, request(2, 100))
        .await
        .unwrap();
    read(&two, &cap, first).await.unwrap();
    two.revoke_read_capability(&alice, cap.capability_id)
        .await
        .unwrap();
    assert!(matches!(
        read(&one, &cap, first).await,
        Err(ArtifactPlaneError::Unauthenticated)
    ));
    let cap = one
        .issue_read_capability(&alice, request(2, 100))
        .await
        .unwrap();
    read(&two, &cap, first).await.unwrap();
    store
        .client()
        .query("UPDATE $record SET updated_at = $changed;")
        .bind(("record", context_id.clone()))
        .bind(("changed", Utc::now() + TimeDelta::seconds(1)))
        .await
        .unwrap()
        .check()
        .unwrap();
    read(&two, &cap, first)
        .await
        .expect("metadata-only context update invalidated delegation");
    store
        .client()
        .query("UPDATE $record SET memberships = [];")
        .bind(("record", context_id))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        matches!(
            read(&two, &cap, first).await,
            Err(ArtifactPlaneError::Unauthenticated)
        ),
        "changed Work Context policy retained delegation"
    );
    drop(one);
    drop(two);
    drop(store);
    database.finish();
    println!("Native read delegation verified across service instances; database process reaped.");
}
