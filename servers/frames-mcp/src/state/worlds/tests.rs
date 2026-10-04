use super::*;
use crate::{
    contract::FrameWorldId,
    state::read_tests::{scope, tree},
    test_store::TestDb,
};
use std::time::Duration;
use veoveo_types::DataLabelId;

fn create_request(name: &str) -> CreateWorldRequest {
    CreateWorldRequest {
        world_id: FrameWorldId::parse(name).unwrap(),
        display_name: "Survey".into(),
        description: Some("A complete world".into()),
    }
}
fn publication(
    name: &str,
    description: &str,
    expected: Option<FrameWorldRevisionId>,
) -> PublishWorldRequest {
    let mut tree = tree();
    tree.frames[0].description = Some(description.into());
    PublishWorldRequest {
        world_id: FrameWorldId::parse(name).unwrap(),
        expected_head_revision_id: expected,
        tree,
    }
}
async fn counts(db: &TestDb) -> (usize, usize) {
    let mut result =
        db.b.client()
            .query("SELECT VALUE id FROM frame_world; SELECT VALUE id FROM frame_world_revision;")
            .await
            .unwrap()
            .check()
            .unwrap();
    (
        result.take::<Vec<RecordId>>(0).unwrap().len(),
        result.take::<Vec<RecordId>>(1).unwrap().len(),
    )
}

#[tokio::test]
async fn concurrent_world_creation_preserves_visible_metadata_replay_and_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let a = FramesState::new(db.a.clone());
        let b = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "tenant", "owner", &["cui"]).await;
        let peer = scope(&db.a, "tenant", "peer", &["cui"]).await;
        let hidden = scope(&db.a, "tenant", "owner", &[]).await;
        let (first, replay) = tokio::join!(
            a.create_world(&owner, create_request("world")),
            b.create_world(&owner, create_request("world"))
        );
        assert_eq!(first.unwrap(), replay.unwrap());
        assert_eq!(counts(&db).await, (1, 0));
        // An identical create is a metadata read within the shared tenant policy.
        b.create_world(&peer, create_request("world"))
            .await
            .unwrap();
        assert!(
            b.create_world(&hidden, create_request("world"))
                .await
                .is_err()
        );
        let mut changed = create_request("world");
        changed.display_name = "Different".into();
        assert!(b.create_world(&owner, changed).await.is_err());
        for name in [" ".to_owned(), "x".repeat(513), "control\n".to_owned()] {
            let mut invalid = create_request("invalid");
            invalid.display_name = name;
            assert!(a.create_world(&owner, invalid).await.is_err());
        }
        for description in [" ".to_owned(), "x".repeat(2049), "control\n".to_owned()] {
            let mut invalid = create_request("invalid");
            invalid.description = Some(description);
            assert!(a.create_world(&owner, invalid).await.is_err());
        }
        let mut oversized = owner.clone();
        oversized
            .data_labels
            .insert(DataLabelId::parse("x".repeat(257)).unwrap());
        assert!(
            a.create_world(&oversized, create_request("invalid"))
                .await
                .is_err()
        );
        assert_eq!(counts(&db).await, (1, 0));
    })
    .await
    .expect("world creation qualification exceeded 90 seconds");
}

#[tokio::test]
async fn concurrent_publication_is_atomic_and_uses_the_expected_head() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let a = FramesState::new(db.a.clone());
        let b = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "tenant", "owner", &[]).await;
        a.create_world(&owner, create_request("world"))
            .await
            .unwrap();
        let request = publication("world", "first", None);
        let (first, replay) = tokio::join!(
            a.publish_world(&owner, request.clone()),
            b.publish_world(&owner, request.clone())
        );
        let first = first.unwrap();
        let replay = replay.unwrap();
        assert_ne!(first.created, replay.created);
        assert_eq!(first.revision, replay.revision);
        assert_eq!(first.world, replay.world);
        assert_eq!(counts(&db).await, (1, 1));
        let restarted = FramesState::new(db.b.clone());
        assert!(
            !restarted
                .publish_world(&owner, request)
                .await
                .unwrap()
                .created
        );
        assert!(
            a.publish_world(&owner, publication("world", "second", None))
                .await
                .is_err()
        );
        let expected = Some(first.revision.revision_id());
        let (left, right) = tokio::join!(
            a.publish_world(&owner, publication("world", "left", expected.clone())),
            b.publish_world(&owner, publication("world", "right", expected))
        );
        assert_ne!(left.is_ok(), right.is_ok());
        let accepted = left.or(right).unwrap();
        assert!(accepted.created);
        assert_eq!(accepted.world.revision(), 2);
        assert_eq!(
            accepted.world.head_revision_id(),
            Some(&accepted.revision.revision_id())
        );
        assert_eq!(
            restarted
                .get_head_revision(&owner, &accepted.world.world_id())
                .await
                .unwrap(),
            Some(accepted.revision)
        );
        assert_eq!(counts(&db).await, (1, 2));
    })
    .await
    .expect("world publication qualification exceeded 90 seconds");
}

#[tokio::test]
async fn publication_and_replay_enforce_current_tenant_owner_and_labels() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let a = FramesState::new(db.a.clone());
        let owner = scope(&db.a, "tenant", "owner", &["cui"]).await;
        a.create_world(&owner, create_request("world"))
            .await
            .unwrap();
        let first = a
            .publish_world(&owner, publication("world", "first", None))
            .await
            .unwrap();
        for denied in [
            scope(&db.a, "tenant", "peer", &["cui"]).await,
            scope(&db.a, "other", "owner", &["cui"]).await,
            scope(&db.a, "tenant", "owner", &[]).await,
        ] {
            assert!(
                a.publish_world(&denied, publication("world", "first", None))
                    .await
                    .is_err()
            );
            assert!(
                a.publish_world(
                    &denied,
                    publication("world", "changed", Some(first.revision.revision_id()))
                )
                .await
                .is_err()
            );
        }
        // Revoke clearance on the stored world after the first successful publication.
        db.a.client()
            .query(
                "UPDATE frame_world SET labels += 'secret' WHERE world_key = 'world' RETURN NONE;",
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            a.publish_world(&owner, publication("world", "first", None))
                .await
                .is_err()
        );
        assert!(
            a.publish_world(
                &owner,
                publication("world", "changed", Some(first.revision.revision_id()))
            )
            .await
            .is_err()
        );
        assert_eq!(counts(&db).await, (1, 1));
    })
    .await
    .expect("world publication authority qualification exceeded 90 seconds");
}

#[tokio::test]
async fn publication_rejects_deleted_or_mismatched_head_parents_without_events() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await; let a = FramesState::new(db.a.clone());
        let owner = scope(&db.a, "tenant", "owner", &[]).await;
        for (i, query) in [
            "UPDATE frame_world_revision SET owner = principal:other WHERE world_key = $world RETURN NONE;",
            "UPDATE frame_world_revision SET tenant = tenant:other WHERE world_key = $world RETURN NONE;",
            "UPDATE frame_world_revision SET world = frame_world:other WHERE world_key = $world RETURN NONE;",
            "UPDATE frame_world_revision SET world_key = 'other' WHERE world_key = $world RETURN NONE;",
            "UPDATE frame_world_revision SET revision_key = 'other' WHERE world_key = $world RETURN NONE;",
            "UPDATE frame_world_revision SET revision = 9 WHERE world_key = $world RETURN NONE;",
            "DELETE frame_world_revision WHERE world_key = $world RETURN NONE;",
            "DELETE frame_world WHERE world_key = $world RETURN NONE;",
        ].iter().enumerate() {
            let name = format!("world-{i}");
            a.create_world(&owner, create_request(&name)).await.unwrap();
            let first = a.publish_world(&owner, publication(&name, "first", None)).await.unwrap();
            db.a.client().query(*query).bind(("world", name.clone())).await.unwrap().check().unwrap();
            let before = counts(&db).await;
            assert!(a.publish_world(&owner, publication(&name, "first", None)).await.is_err(), "{query}");
            assert!(a.publish_world(&owner, publication(&name, "changed", Some(first.revision.revision_id()))).await.is_err(), "{query}");
            assert_eq!(counts(&db).await, before);
        }
    }).await.expect("world head qualification exceeded 90 seconds");
}

#[tokio::test]
async fn domain_failure_rolls_back_creation_revision_and_head_and_allows_retry() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await; let a = FramesState::new(db.a.clone());
        let owner = scope(&db.a, "tenant", "owner", &[]).await;
        db.a.client().query("DEFINE EVENT fail_world ON frame_world WHEN $event = 'CREATE' THEN { THROW 'injected creation event failure'; };").await.unwrap().check().unwrap();
        assert!(a.create_world(&owner, create_request("world")).await.is_err());
        assert_eq!(counts(&db).await, (0,0));
        db.a.client().query("REMOVE EVENT IF EXISTS fail_world ON frame_world; REMOVE EVENT IF EXISTS fail_world ON frame_world_revision;").await.unwrap().check().unwrap();
        a.create_world(&owner, create_request("world")).await.unwrap();
        for query in [
            "DEFINE EVENT fail_world ON frame_world_revision WHEN $event = 'CREATE' THEN { THROW 'injected publication event failure'; };",
            "DEFINE EVENT fail_world ON frame_world WHEN $event = 'UPDATE' THEN { THROW 'injected publication event failure'; };",
        ] {
            db.a.client().query(query).await.unwrap().check().unwrap();
            assert!(a.publish_world(&owner, publication("world", "first", None)).await.is_err());
            assert_eq!(counts(&db).await, (1,0));
            let world = a.get_world(&owner, &FrameWorldId::parse("world").unwrap()).await.unwrap().unwrap();
            assert_eq!(world.revision(), 0); assert!(world.head_revision_id().is_none());
            db.a.client().query("REMOVE EVENT IF EXISTS fail_world ON frame_world; REMOVE EVENT IF EXISTS fail_world ON frame_world_revision;").await.unwrap().check().unwrap();
        }
        assert!(a.publish_world(&owner, publication("world", "first", None)).await.unwrap().created);
        assert_eq!(counts(&db).await, (1,1));
    }).await.expect("world transaction rollback qualification exceeded 90 seconds");
}
