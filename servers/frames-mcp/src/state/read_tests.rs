//! Native authorization and parent-integrity cases use two independent clients
//! against the shared fixture's isolated, pinned SurrealDB process.
use super::*;
use crate::{
    contract::{
        CreateWorldRequest, FrameBasis, FrameId, FrameNode, FrameParentTransform, FrameWorldTree,
        PublishWorldRequest,
    },
    test_store::TestDb,
};
use std::time::Duration;
use veoveo_platform_store::PrincipalKind;

pub(super) async fn scope(
    store: &PlatformStore,
    tenant: &str,
    principal: &str,
    labels: &[&str],
) -> FrameScope {
    FrameScope {
        identity: store
            .ensure_identity(
                tenant,
                principal,
                "https://fixture.local",
                principal,
                PrincipalKind::Service,
            )
            .await
            .unwrap(),
        data_labels: labels
            .iter()
            .map(|value| DataLabelId::parse(*value).unwrap())
            .collect(),
    }
}

pub(super) fn tree() -> FrameWorldTree {
    FrameWorldTree {
        frames: vec![
            FrameNode {
                frame_id: FrameId::parse("root").unwrap(),
                basis: FrameBasis::EcefWgs84,
                parent_frame_id: None,
                parent_transform: None,
                description: None,
            },
            FrameNode {
                frame_id: FrameId::parse("vehicle").unwrap(),
                basis: FrameBasis::Frd,
                parent_frame_id: Some(FrameId::parse("root").unwrap()),
                parent_transform: Some(FrameParentTransform::StaticRigid {
                    translation_m: [1., 2., 3.],
                    rotation_xyzw: [0., 0., 0., 1.],
                }),
                description: None,
            },
        ],
    }
}

pub(super) async fn create(state: &FramesState, scope: &FrameScope, name: &str) -> FrameWorldId {
    let world_id = FrameWorldId::parse(name).unwrap();
    state
        .create_world(
            scope,
            CreateWorldRequest {
                world_id: world_id.clone(),
                display_name: name.to_owned(),
                description: None,
            },
        )
        .await
        .unwrap();
    world_id
}

async fn publish(
    state: &FramesState,
    scope: &FrameScope,
    world_id: &FrameWorldId,
) -> FrameWorldRevision {
    state
        .publish_world(
            scope,
            PublishWorldRequest {
                world_id: world_id.clone(),
                expected_head_revision_id: None,
                tree: tree(),
            },
        )
        .await
        .unwrap()
        .revision
}

async fn assert_revision_visible(
    state: &FramesState,
    scope: &FrameScope,
    revision: &FrameWorldRevision,
    visible: bool,
) {
    assert_eq!(
        state
            .get_revision(scope, revision.revision_uri())
            .await
            .unwrap()
            .is_some(),
        visible
    );
    assert_eq!(
        state
            .get_head_revision(scope, &revision.world_id())
            .await
            .unwrap()
            .is_some(),
        visible
    );
    assert_eq!(
        state
            .get_frame(scope, &revision.root_frame_uri())
            .await
            .unwrap()
            .is_some(),
        visible
    );
}

#[tokio::test]
async fn native_world_reads_apply_current_tenant_and_all_labels_in_sql() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_modules(vec![
            crate::schema::module_setup(
                crate::test_store::module_lanes::execution("frames").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let writer = FramesState::new(db.a.clone());
        let reader = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "frames-a", "owner", &["cui", "mission"]).await;
        let peer = scope(&db.b, "frames-a", "peer", &["cui", "mission", "extra"]).await;
        let partial = scope(&db.b, "frames-a", "partial", &["cui"]).await;
        let public = scope(&db.a, "frames-a", "public", &[]).await;
        let foreign = scope(&db.b, "frames-b", "owner", &["cui", "mission"]).await;
        let world_id = create(&writer, &owner, "classified").await;
        assert!(
            reader
                .get_head_revision(&owner, &world_id)
                .await
                .unwrap()
                .is_none()
        );
        let revision = publish(&writer, &owner, &world_id).await;
        let public_id = create(&writer, &public, "public").await;
        create(&writer, &foreign, "classified").await;
        for allowed in [&owner, &peer] {
            assert!(
                reader
                    .get_world(allowed, &world_id)
                    .await
                    .unwrap()
                    .is_some()
            );
            assert_revision_visible(&reader, allowed, &revision, true).await;
            assert_eq!(
                reader.worlds_page(allowed, None).await.unwrap().items.len(),
                2
            );
        }
        for denied in [&partial, &public] {
            assert!(reader.get_world(denied, &world_id).await.unwrap().is_none());
            assert_revision_visible(&reader, denied, &revision, false).await;
            let worlds = reader.worlds_page(denied, None).await.unwrap().items;
            assert_eq!(worlds.len(), 1);
            assert_eq!(worlds[0].world_id(), public_id);
        }
        assert_revision_visible(&reader, &foreign, &revision, false).await;
        assert!(
            reader
                .get_world(&foreign, &public_id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            reader
                .worlds_page(&foreign, None)
                .await
                .unwrap()
                .items
                .len(),
            1
        );

        // Same-tenant readers need not own a world, but publication still does.
        assert!(
            writer
                .publish_world(
                    &peer,
                    PublishWorldRequest {
                        world_id: world_id.clone(),
                        expected_head_revision_id: Some(revision.revision_id().clone()),
                        tree: tree(),
                    }
                )
                .await
                .is_err()
        );

        // Visibility follows current labels even for immutable revision addresses.
        db.a.client()
            .query(include_str!("../../tests/queries/restrict_world.surql"))
            .bind(("tenant", owner.identity.tenant_id.record_id()))
            .bind(("world_key", world_id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_revision_visible(&reader, &owner, &revision, false).await;
        assert!(reader.get_world(&owner, &world_id).await.unwrap().is_none());
        assert_eq!(
            reader.worlds_page(&owner, None).await.unwrap().items.len(),
            1
        );
        let mut newly_cleared = owner.clone();
        newly_cleared
            .data_labels
            .insert(DataLabelId::parse("restricted").unwrap());
        assert_revision_visible(&reader, &newly_cleared, &revision, true).await;
    })
    .await
    .expect("world visibility qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_revision_reads_reject_wrong_or_deleted_parents() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_modules(vec![
            crate::schema::module_setup(
                crate::test_store::module_lanes::execution("frames").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let writer = FramesState::new(db.a.clone());
        let reader = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "frames-a", "owner", &[]).await;
        let foreign = scope(&db.a, "frames-b", "owner", &[]).await;
        let world_id = create(&writer, &owner, "world").await;
        let other_id = create(&writer, &owner, "other").await;
        create(&writer, &foreign, "world").await;
        let revision = publish(&writer, &owner, &world_id).await;
        assert_revision_visible(&reader, &owner, &revision, true).await;
        let wrong_uri = FrameWorldRevisionUri::new(&other_id, &revision.revision_id());
        assert!(
            reader
                .get_revision(&owner, &wrong_uri)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .get_frame(
                    &owner,
                    &WorldFrameUri::new(&wrong_uri, &FrameId::parse("root").unwrap())
                )
                .await
                .unwrap()
                .is_none()
        );

        // Retained/corrupted copied keys cannot authorize a different record link.
        for (parent_scope, parent_world) in [(&owner, &other_id), (&foreign, &world_id)] {
            db.a.client()
                .query(include_str!(
                    "../../tests/queries/replace_revision_parent.surql"
                ))
                .bind(("parent_tenant", parent_scope.identity.tenant_id.record_id()))
                .bind(("parent_key", parent_world.to_string()))
                .bind(("tenant", owner.identity.tenant_id.record_id()))
                .bind(("revision_key", revision.revision_id().to_string()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert_revision_visible(&reader, &owner, &revision, false).await;
        }
        db.a.client()
            .query(include_str!(
                "../../tests/queries/restore_revision_parent.surql"
            ))
            .bind(("tenant", owner.identity.tenant_id.record_id()))
            .bind(("world_key", world_id.to_string()))
            .bind(("revision_key", revision.revision_id().to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_revision_visible(&reader, &owner, &revision, true).await;
        db.a.client()
            .query(include_str!("../../tests/queries/delete_world.surql"))
            .bind(("tenant", owner.identity.tenant_id.record_id()))
            .bind(("world_key", world_id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_revision_visible(&reader, &owner, &revision, false).await;
        assert!(reader.get_world(&owner, &world_id).await.unwrap().is_none());
    })
    .await
    .expect("revision parent qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_head_reads_require_consistent_pointer_key_and_revision() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_modules(vec![
            crate::schema::module_setup(
                crate::test_store::module_lanes::execution("frames").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let writer = FramesState::new(db.a.clone());
        let reader = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "frames-a", "owner", &[]).await;
        let world_id = create(&writer, &owner, "world").await;
        let old = publish(&writer, &owner, &world_id).await;
        let mut revised = tree();
        revised.frames[0].description = Some("second revision".to_owned());
        let new = writer
            .publish_world(
                &owner,
                PublishWorldRequest {
                    world_id: world_id.clone(),
                    expected_head_revision_id: Some(old.revision_id().clone()),
                    tree: revised,
                },
            )
            .await
            .unwrap()
            .revision;
        assert_eq!(
            reader.get_head_revision(&owner, &world_id).await.unwrap(),
            Some(new.clone())
        );
        assert_eq!(
            reader
                .get_revision(&owner, old.revision_uri())
                .await
                .unwrap(),
            Some(old.clone())
        );
        // Each independently inconsistent head representation fails closed.
        for query in [
            include_str!("../../tests/queries/point_head_to_old_key.surql"),
            include_str!("../../tests/queries/mismatch_head_revision_number.surql"),
            include_str!("../../tests/queries/point_head_to_old_revision.surql"),
        ] {
            db.a.client()
                .query(query)
                .bind(("tenant", owner.identity.tenant_id.record_id()))
                .bind(("world_key", world_id.to_string()))
                .bind(("old_key", old.revision_id().to_string()))
                .bind(("new_key", new.revision_id().to_string()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                reader
                    .get_head_revision(&owner, &world_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                reader
                    .get_revision(&owner, new.revision_uri())
                    .await
                    .unwrap(),
                Some(new.clone())
            );
        }
    })
    .await
    .expect("head consistency qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_frame_reads_select_only_the_requested_node() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_modules(vec![
            crate::schema::module_setup(
                crate::test_store::module_lanes::execution("frames").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let writer = FramesState::new(db.a.clone());
        let reader = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "frames-a", "owner", &[]).await;
        let world_id = create(&writer, &owner, "world").await;
        let revision = publish(&writer, &owner, &world_id).await;
        for node in &revision.tree().frames {
            let uri = WorldFrameUri::new(revision.revision_uri(), &node.frame_id);
            assert_eq!(
                reader.get_frame(&owner, &uri).await.unwrap(),
                Some(node.clone())
            );
        }
        let missing =
            WorldFrameUri::new(revision.revision_uri(), &FrameId::parse("missing").unwrap());
        assert!(reader.get_frame(&owner, &missing).await.unwrap().is_none());
        let root = revision.frame(&revision.root_frame_uri()).unwrap();
        // An invalid unrelated node proves resource selection happens in SQL,
        // before decoding a full tree at the application boundary.
        for unrelated in [
            serde_json::json!({"frame_id": "bad", "basis": {"kind": "invalid"}}),
            serde_json::to_value(root).unwrap(),
        ] {
            let definition =
                object_from_value(serde_json::json!({"frames": [root, unrelated]})).unwrap();
            db.a.client()
                .query(include_str!(
                    "../../tests/queries/replace_revision_definition.surql"
                ))
                .bind(("definition", definition))
                .bind(("tenant", owner.identity.tenant_id.record_id()))
                .bind(("revision_key", revision.revision_id().to_string()))
                .await
                .unwrap()
                .check()
                .unwrap();
            if unrelated["frame_id"] == "bad" {
                assert_eq!(
                    reader
                        .get_frame(&owner, &revision.root_frame_uri())
                        .await
                        .unwrap(),
                    Some(root.clone())
                );
                assert!(
                    reader
                        .get_revision(&owner, revision.revision_uri())
                        .await
                        .is_err()
                );
            } else {
                assert!(
                    reader
                        .get_frame(&owner, &revision.root_frame_uri())
                        .await
                        .is_err()
                );
            }
        }
    })
    .await
    .expect("frame selection qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_revision_admission_rejects_stored_root_digest_and_tree_mismatch() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_modules(vec![
            crate::schema::module_setup(
                crate::test_store::module_lanes::execution("frames").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let writer = FramesState::new(db.a.clone());
        let reader = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "frames-a", "owner", &["private"]).await;
        let denied = scope(&db.a, "frames-a", "reader", &[]).await;
        let world_id = create(&writer, &owner, "world").await;
        let revision = publish(&writer, &owner, &world_id).await;
        for query in [
            include_str!("../../tests/queries/corrupt_revision_root.surql"),
            include_str!("../../tests/queries/corrupt_revision_digest.surql"),
            include_str!("../../tests/queries/tamper_revision_definition.surql"),
        ] {
            db.a.client()
                .query(query)
                .bind(("tenant", owner.identity.tenant_id.record_id()))
                .bind(("revision_key", revision.revision_id().to_string()))
                .bind(("root", revision.root_frame_uri().frame_id().to_string()))
                .bind(("digest", revision.spec_digest().hex().to_owned()))
                .bind(("wrong_digest", "0".repeat(64)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                reader
                    .get_revision(&denied, revision.revision_uri())
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                reader
                    .get_revision(&owner, revision.revision_uri())
                    .await
                    .is_err()
            );
            assert!(reader.get_head_revision(&owner, &world_id).await.is_err());
        }
    })
    .await
    .expect("stored revision integrity qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_dynamic_references_round_trip_and_reject_malformed_retained_nodes() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::with_modules(vec![
            crate::schema::module_setup(
                crate::test_store::module_lanes::execution("frames").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let writer = FramesState::new(db.a.clone());
        let reader = FramesState::new(db.b.clone());
        let owner = scope(&db.a, "frames-a", "owner", &["private"]).await;
        let denied = scope(&db.b, "frames-a", "reader", &[]).await;
        let world_id = create(&writer, &owner, "dynamic").await;
        let mut tree = tree();
        tree.frames[1].parent_transform = Some(FrameParentTransform::DynamicStream {
            stream_uri: "uav-sim://session/showcase".parse().unwrap(),
            entity_path: "/world/vehicle/body".parse().unwrap(),
        });
        let revision = writer
            .publish_world(
                &owner,
                PublishWorldRequest {
                    world_id: world_id.clone(),
                    expected_head_revision_id: None,
                    tree,
                },
            )
            .await
            .unwrap()
            .revision;
        assert_eq!(
            reader
                .get_revision(&owner, revision.revision_uri())
                .await
                .unwrap(),
            Some(revision.clone())
        );
        let node = revision
            .tree()
            .frames
            .iter()
            .find(|frame| frame.frame_id.as_str() == "vehicle")
            .unwrap();
        let node_uri = WorldFrameUri::new(revision.revision_uri(), &node.frame_id);
        assert_eq!(
            reader.get_frame(&owner, &node_uri).await.unwrap(),
            Some(node.clone())
        );
        let baseline = serde_json::to_value(revision.tree()).unwrap();
        let index = revision
            .tree()
            .frames
            .iter()
            .position(|frame| frame.frame_id == node.frame_id)
            .unwrap();
        for (field, value) in [
            ("stream_uri", "uav-sim://session/{session_id}"),
            ("entity_path", "body\nposition"),
        ] {
            let mut definition = baseline.clone();
            definition["frames"][index]["parent_transform"][field] = value.into();
            db.a.client()
                .query(include_str!(
                    "../../tests/queries/replace_revision_definition.surql"
                ))
                .bind(("definition", object_from_value(definition.clone()).unwrap()))
                .bind(("tenant", owner.identity.tenant_id.record_id()))
                .bind(("revision_key", revision.revision_id().to_string()))
                .await
                .unwrap()
                .check()
                .unwrap();
            // Denied callers never reach decoding, while admitted reads reject the value.
            assert!(
                reader
                    .get_revision(&denied, revision.revision_uri())
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                reader
                    .get_frame(&denied, &node_uri)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                reader
                    .get_revision(&owner, revision.revision_uri())
                    .await
                    .is_err()
            );
            assert!(reader.get_head_revision(&owner, &world_id).await.is_err());
            assert!(reader.get_frame(&owner, &node_uri).await.is_err());
            assert!(
                reader
                    .get_frame(&owner, &revision.root_frame_uri())
                    .await
                    .unwrap()
                    .is_some()
            );
            let retained: Vec<FrameWorldRevisionRecord> =
                db.b.client()
                    .query(include_str!(
                        "../../tests/queries/read_revision_record.surql"
                    ))
                    .bind(("tenant", owner.identity.tenant_id.record_id()))
                    .bind(("revision_key", revision.revision_id().to_string()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap()
                    .take(0)
                    .unwrap();
            assert_eq!(
                serde_json::to_value(&retained[0].definition).unwrap(),
                definition
            );
        }
    })
    .await
    .expect("dynamic reference qualification exceeded 90 seconds");
}
