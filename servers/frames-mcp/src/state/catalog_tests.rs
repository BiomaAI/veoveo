use super::{
    FramesState,
    read_tests::{create, scope, tree},
};
use crate::{
    contract::{
        FrameId, FrameNode, FrameWorldRevisionId, FrameWorldRevisionUri, PublishWorldRequest,
    },
    test_store::TestDb,
};
use std::time::Duration;

#[tokio::test]
async fn native_world_pages_and_completion_filter_before_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let writer = FramesState::new(db.a.clone());
        let reader = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "world-pages", "author", &["cui"]).await;
        let denied_labels = scope(&db.a, "world-pages", "author", &["cui", "mission"]).await;
        let foreign = scope(&db.a, "other-tenant", "author", &["cui"]).await;
        for i in 0..110 {
            create(&writer, &denied_labels, &format!("a-{i:03}")).await;
            create(&writer, &foreign, &format!("a-{i:03}")).await;
        }
        let mut expected = Vec::new();
        for i in 0..125 {
            expected.push(create(&writer, &owner, &format!("z-{i:03}")).await);
        }
        let matching = create(&writer, &owner, "zz-Needle").await;
        expected.push(matching.clone());
        let first = reader.worlds_page(&owner, None).await.unwrap();
        assert_eq!(first.limit, 100);
        assert_eq!(first.items.len(), 100);
        let cursor = first.next_cursor.as_ref().unwrap();
        assert_eq!(cursor.after(), &expected[99]);
        let second = reader.worlds_page(&owner, Some(cursor)).await.unwrap();
        assert_eq!(second.items.len(), 26);
        assert!(second.next_cursor.is_none());
        assert_eq!(first.items.iter().chain(second.items.iter()).map(|world| world.world_id().clone()).collect::<Vec<_>>(), expected);
        assert_eq!(reader.complete_worlds(&owner, "NEEDLE").await.unwrap(), vec![matching]);
        let completions = reader.complete_worlds(&owner, "").await.unwrap();
        assert_eq!(completions, expected[..101]);
        assert!(reader.complete_worlds(&owner, "a-").await.unwrap().is_empty());
        assert!(reader.complete_worlds(&owner, "\n").await.is_err());
        assert!(reader.complete_worlds(&owner, &"x".repeat(513)).await.is_err());
        // Cursors carry a key, not authority for the identity that minted them.
        assert!(reader.worlds_page(&foreign, Some(cursor)).await.unwrap().items.is_empty());
        db.a.client().query("UPDATE frame_world SET labels = ['cui', 'mission'] WHERE tenant = $tenant AND world_key > $after RETURN NONE;")
            .bind(("tenant", owner.identity.tenant_id.record_id()))
            .bind(("after", cursor.after().to_string()))
            .await.unwrap().check().unwrap();
        let revoked = reader.worlds_page(&owner, Some(cursor)).await.unwrap();
        assert!(revoked.items.is_empty());
        assert!(revoked.next_cursor.is_none());
        assert!(reader.complete_worlds(&owner, "needle").await.unwrap().is_empty());
        assert_eq!(reader.worlds_page(&denied_labels, Some(cursor)).await.unwrap().items.len(), 26);
    }).await.expect("world catalog qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_revision_and_frame_completions_bind_visible_parents_before_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let writer = FramesState::new(db.a.clone());
        let reader = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "completions", "owner", &["cui"]).await;
        let denied = scope(&db.a, "completions", "peer", &[]).await;
        let foreign = scope(&db.a, "other-completions", "owner", &["cui"]).await;
        let world = create(&writer, &owner, "world").await;
        let other = create(&writer, &owner, "other").await;
        let mut expected = Vec::new();
        let mut head = None;
        // Real publications give each revision a distinct complete tree and digest.
        for i in 0..125 {
            let mut tree = tree();
            tree.frames[0].description = Some(format!("revision {i}"));
            let child = tree.frames[1].clone();
            for n in 0..125 {
                tree.frames.push(FrameNode {
                    frame_id: FrameId::parse(format!("node-{n:03}")).unwrap(),
                    ..child.clone()
                });
            }
            tree.frames.push(FrameNode {
                frame_id: FrameId::parse("zz-Needle").unwrap(),
                ..child
            });
            let revision = writer
                .publish_world(
                    &owner,
                    PublishWorldRequest {
                        world_id: world.clone(),
                        expected_head_revision_id: head.clone(),
                        tree,
                    },
                )
                .await
                .unwrap()
                .revision;
            expected.push(revision.revision_id().clone());
            head = Some(revision.revision_id());
        }
        let revision_id = head.unwrap();
        let revision = FrameWorldRevisionUri::new(&world, &revision_id);
        assert_eq!(
            reader.complete_revisions(&owner, &world, "").await.unwrap(),
            expected[..101]
        );
        assert_eq!(
            reader
                .complete_revisions(&owner, &world, &revision_id.to_string().to_uppercase())
                .await
                .unwrap(),
            vec![revision_id.clone()]
        );
        assert_eq!(
            reader
                .complete_frames(&owner, &revision, "NEEDLE")
                .await
                .unwrap(),
            vec![FrameId::parse("zz-Needle").unwrap()]
        );
        assert_eq!(
            reader
                .complete_frames(&owner, &revision, "")
                .await
                .unwrap()
                .len(),
            101
        );
        for context in [&denied, &foreign] {
            assert!(
                reader
                    .complete_revisions(context, &world, "")
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert!(
                reader
                    .complete_frames(context, &revision, "")
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
        let wrong_world = FrameWorldRevisionUri::new(&other, &revision_id);
        let missing_revision =
            FrameWorldRevisionUri::new(&world, &FrameWorldRevisionId::parse("missing").unwrap());
        for wrong in [&wrong_world, &missing_revision] {
            assert!(
                reader
                    .complete_frames(&owner, wrong, "")
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            reader
                .complete_revisions(&owner, &other, "")
                .await
                .unwrap()
                .is_empty()
        );
        db.a.client()
            .query("DELETE frame_world WHERE tenant = $tenant AND world_key = $world RETURN NONE;")
            .bind(("tenant", owner.identity.tenant_id.record_id()))
            .bind(("world", world.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            reader
                .complete_revisions(&owner, &world, "")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            reader
                .complete_frames(&owner, &revision, "")
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await
    .expect("parent completion qualification exceeded 90 seconds");
}
