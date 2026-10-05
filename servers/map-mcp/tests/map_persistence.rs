use chrono::Utc;
use uuid::Uuid;
use veoveo_map_mcp::persistence::*;
use veoveo_platform_store::*;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "map_persistence/projection.rs"]
mod map_projection;
fn artifact_authority(identity: &PlatformIdentity) -> InvocationAuthorityRecord {
    InvocationAuthorityRecord {
        context_key: "operations".into(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: "r1".into(),
        owner_kind: ArtifactGrantSubjectKind::Principal,
        owner_key: identity.principal_key.clone(),
        initial_grants: vec![WorkContextInitialGrantRecord {
            subject_kind: ArtifactGrantSubjectKind::Principal,
            subject_key: identity.principal_key.clone(),
            permission: GrantPermission::Admin,
        }],
        classification: None,
        data_labels: Vec::new(),
        invocation_mode: InvocationMode::Direct,
        initiator_key: Some(identity.principal_key.clone()),
        delegation_id: None,
    }
}

#[tokio::test]
async fn authored_map_changes_commit_atomically_and_replay_idempotently() {
    tokio::time::timeout(std::time::Duration::from_secs(180), async {
        let db = fixture::TestDb::with_modules(vec![
            veoveo_map_mcp::schema::module_setup(fixture::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let platform = db.a.clone();
        let store = MapRepository::new(platform.clone());
        let identity = platform
            .ensure_identity(
                "tenant-map-authoring",
                "map-author",
                "https://veoveo.local/services",
                "map-author",
                PrincipalKind::Service,
            )
            .await
            .unwrap();
        let authority = artifact_authority(&identity);
        let layer_key = format!("feature-layer-{}", Uuid::now_v7());
        store
            .create_map_feature_layer(MapFeatureLayerDraft {
                identity: identity.clone(),
                authority: authority.clone(),
                layer_key: layer_key.clone(),
                title: "Inspection areas".to_owned(),
                description: None,
                content_class: "boundaries".to_owned(),
                schema: MapFeatureSchemaDraft {
                    schema_revision_key: format!("feature-schema-{}", Uuid::now_v7()),
                    schema_version: 1,
                    digest_sha256: "a".repeat(64),
                    schema_json: r#"{"type":"object"}"#.to_owned(),
                },
                style: None,
                revision: 0,
                archived_at: None,
                canonical_json: r#"{"revision":0}"#.to_owned(),
            })
            .await
            .unwrap();

        let changeset_key = format!("changeset-{}", Uuid::now_v7());
        let feature_key = format!("feature-{}", Uuid::now_v7());
        let draft = MapFeatureCommitDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            layer_key: layer_key.clone(),
            layer_canonical_json: r#"{"revision":1}"#.to_owned(),
            expected_layer_revision: 0,
            changeset_key: changeset_key.clone(),
            idempotency_key: "first-inspection-area".to_owned(),
            request_digest_sha256: "b".repeat(64),
            changeset_canonical_json: r#"{"resulting_layer_revision":1}"#.to_owned(),
            revisions: vec![MapFeatureRevisionDraft {
                feature_key: feature_key.clone(),
                feature_revision: 1,
                layer_revision: 1,
                schema_version: 1,
                deleted: false,
                geometry_type: "Point".to_owned(),
                geometry_json: r#"{"type":"Point","coordinates":[-89.2,13.7]}"#.to_owned(),
                bbox_west: -89.2,
                bbox_south: 13.7,
                bbox_east: -89.2,
                bbox_north: 13.7,
                valid_from: None,
                valid_until: None,
                semantic_type: "inspection_area".to_owned(),
                title: Some("Area A".to_owned()),
                canonical_json: r#"{"type":"Feature"}"#.to_owned(),
                expected_feature_revision: None,
            }],
        };
        let committed = store
            .commit_map_feature_changes(draft.clone())
            .await
            .unwrap();
        assert!(committed.changeset.commit_sequence > 0);
        assert_eq!(committed.revisions.len(), 1);
        map_projection::current_catalog_replays_commits(
            &store,
            committed.changeset.commit_sequence,
        )
        .await;
        assert_eq!(
            store
                .count_map_feature_heads("tenant-map-authoring", "operations", &layer_key)
                .await
                .unwrap(),
            1
        );
        let replay = store
            .commit_map_feature_changes(draft.clone())
            .await
            .unwrap();
        assert_eq!(replay.changeset, committed.changeset);
        assert_eq!(replay.revisions, committed.revisions);

        let publication_key = format!("publication-{}", Uuid::now_v7());
        store
            .create_map_layer_publication(MapLayerPublicationDraft {
                identity: identity.clone(),
                authority: authority.clone(),
                publication_key: publication_key.clone(),
                layer_key: layer_key.clone(),
                layer_revision: 1,
                schema_version: 1,
                style_revision_key: None,
                artifact_uris: Vec::new(),
                canonical_json: serde_json::json!({
                    "publication_id": publication_key,
                    "layer_id": layer_key,
                    "layer_revision": 1
                })
                .to_string(),
                published_at: Utc::now(),
            })
            .await
            .unwrap();
        let product_key = format!("product-{}", Uuid::now_v7());
        let product = MapLayerProductDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            product_key: product_key.clone(),
            publication_key: publication_key.clone(),
            layer_key: layer_key.clone(),
            layer_revision: 1,
            format: "geojson_seq".to_owned(),
            artifact_uri: veoveo_artifact_contract::ArtifactId::new().plane_uri(),
            mime_type: "application/geo+json-seq".to_owned(),
            digest_sha256: "d".repeat(64),
            size_bytes: 128,
            feature_count: 1,
            canonical_json: serde_json::json!({
                "product_id": product_key,
                "publication_id": publication_key
            })
            .to_string(),
            created_by_key: identity.principal_key.clone(),
            created_at: Utc::now(),
        };
        let created_product = store
            .create_map_layer_product(product.clone())
            .await
            .unwrap();
        let replayed_product = store.create_map_layer_product(product).await.unwrap();
        assert_eq!(created_product, replayed_product);

        let composition_key = format!("composition-{}", Uuid::now_v7());
        let composition = store
            .create_map_composition(MapCompositionDraft {
                identity: identity.clone(),
                authority: authority.clone(),
                composition_key: composition_key.clone(),
                title: "Inspection map".to_owned(),
                revision: MapCompositionRevisionDraft {
                    composition_revision_key: format!("composition-revision-{}", Uuid::now_v7()),
                    revision: 1,
                    publication_keys: vec![publication_key.clone()],
                    canonical_json: serde_json::json!({"revision": 1}).to_string(),
                },
                canonical_json: serde_json::json!({"current_revision": 1}).to_string(),
            })
            .await
            .unwrap();
        assert_eq!(composition.current_revision, 1);
        let updated = store
            .update_map_composition(
                MapCompositionUpdateDraft {
                    identity: identity.clone(),
                    authority: authority.clone(),
                    composition_key: composition_key.clone(),
                    title: "Inspection map".to_owned(),
                    revision: MapCompositionRevisionDraft {
                        composition_revision_key: format!(
                            "composition-revision-{}",
                            Uuid::now_v7()
                        ),
                        revision: 2,
                        publication_keys: vec![publication_key],
                        canonical_json: serde_json::json!({"revision": 2}).to_string(),
                    },
                    canonical_json: serde_json::json!({"current_revision": 2}).to_string(),
                    archived_at: None,
                },
                1,
            )
            .await
            .unwrap();
        assert_eq!(updated.current_revision, 2);
        assert!(
            store
                .map_composition_revision("tenant-map-authoring", "operations", &composition_key, 1)
                .await
                .unwrap()
                .is_some()
        );

        let mut conflicting = draft;
        conflicting.request_digest_sha256 = "c".repeat(64);
        assert!(matches!(
            store.commit_map_feature_changes(conflicting).await,
            Err(MapStoreError::MapRecordConflict { .. })
        ));
    })
    .await
    .expect("owned persistence qualification exceeded 180 seconds");
}

#[tokio::test]
async fn map_release_activation_is_atomic_and_version_guarded() {
    tokio::time::timeout(std::time::Duration::from_secs(180), async {
        let db = fixture::TestDb::with_modules(vec![
            veoveo_map_mcp::schema::module_setup(fixture::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let platform = db.a.clone();
        let store = MapRepository::new(platform.clone());
        let identity = platform
            .ensure_identity(
                "tenant-map",
                "map-admin",
                "https://veoveo.local/services",
                "map-admin",
                PrincipalKind::Service,
            )
            .await
            .unwrap();
        let dataset_key = format!("dataset-{}", Uuid::now_v7());
        let source_key = format!("source-{}", Uuid::now_v7());
        let create_release = |release_key: String| MapReleaseDraft {
            identity: identity.clone(),
            release_key,
            dataset_key: dataset_key.clone(),
            source_key: source_key.clone(),
            state: MapReleaseState::Staged,
            version_label: format!("sha256:{}", "a".repeat(64)),
            source_digest_sha256: "a".repeat(64),
            valid_from: Utc::now(),
            valid_until: None,
            canonical_json: serde_json::json!({ "state": "staged" }).to_string(),
        };

        let first_key = format!("release-{}", Uuid::now_v7());
        store
            .create_map_release(create_release(first_key.clone()))
            .await
            .unwrap();
        let first = store
            .activate_map_release(
                &identity,
                &dataset_key,
                &first_key,
                None,
                1,
                serde_json::json!({ "state": "active" }).to_string(),
            )
            .await
            .unwrap();
        assert_eq!(first.state, MapReleaseState::Active);
        assert_eq!(first.record_version, 2);
        assert_eq!(
            store
                .active_map_release(identity.tenant_id, &dataset_key)
                .await
                .unwrap()
                .unwrap()
                .record_version,
            1
        );

        let second_key = format!("release-{}", Uuid::now_v7());
        store
            .create_map_release(create_release(second_key.clone()))
            .await
            .unwrap();
        let conflict = store
            .activate_map_release(
                &identity,
                &dataset_key,
                &second_key,
                Some(1),
                2,
                serde_json::json!({ "state": "active" }).to_string(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(conflict, MapStoreError::MapRecordConflict { .. }),
            "unexpected activation error: {conflict:?}"
        );
        let second = store
            .map_release(identity.tenant_id, &second_key)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.state, MapReleaseState::Staged);
        assert_eq!(second.record_version, 1);
        let pointer = store
            .active_map_release(identity.tenant_id, &dataset_key)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pointer.release_key, first_key);
        assert_eq!(pointer.record_version, 1);
    })
    .await
    .expect("owned persistence qualification exceeded 180 seconds");
}
