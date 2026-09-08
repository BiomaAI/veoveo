use std::collections::BTreeMap;

use secrecy::SecretString;
use tempfile::TempDir;
use uuid::Uuid;
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, InvocationAuthorityRecord, InvocationMode, MapFeatureCommitDraft,
    MapFeatureLayerDraft, MapFeatureRevisionDraft, MapFeatureSchemaDraft, OpenObject, OutboxDraft,
    PrincipalKind, StoreConfig, StoreCredentials, WorkContextMembershipLevel,
};

use crate::{analytics::MapAnalyticsConfig, contract::*};

use super::*;

#[tokio::test]
async fn recovery_pages_map_commits_and_resumes_the_persisted_projection() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let extension = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION")
        .expect("Map recovery integration requires the pinned Spatial extension");
    let store = PlatformStore::connect(
        StoreConfig::builder(
            std::env::var("VEOVEO_SURREAL_URL").expect("test database endpoint"),
            "veoveo_integration",
            format!("map_recovery_{}", Uuid::now_v7().simple()),
            StoreCredentials::root(
                std::env::var("VEOVEO_SURREAL_USER").expect("test database user"),
                SecretString::from(
                    std::env::var("VEOVEO_SURREAL_PASSWORD").expect("test password"),
                ),
            ),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    let identity = store
        .ensure_identity(
            "map-recovery",
            "author",
            "https://veoveo.local/services",
            "author",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let authority = InvocationAuthorityRecord {
        context_key: "operations".into(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: "r1".into(),
        owner_kind: ArtifactGrantSubjectKind::Principal,
        owner_key: identity.principal_key.clone(),
        initial_grants: vec![],
        classification: None,
        data_labels: vec![],
        invocation_mode: InvocationMode::Direct,
        initiator_key: None,
        delegation_id: None,
    };
    let layer_id = FeatureLayerId::new();
    let feature_id = MapFeatureId::new();
    store
        .create_map_feature_layer(MapFeatureLayerDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            layer_key: layer_id.to_string(),
            title: "Recovery".into(),
            description: None,
            content_class: "boundaries".into(),
            schema: MapFeatureSchemaDraft {
                schema_revision_key: FeatureSchemaRevisionId::new().to_string(),
                schema_version: 1,
                digest_sha256: "a".repeat(64),
                schema_json: "{}".into(),
            },
            style: None,
            revision: 0,
            archived_at: None,
            canonical_json: "{}".into(),
        })
        .await
        .unwrap();
    let feature = MapFeature {
        feature_type: GeoJsonFeatureType::Feature,
        conforms_to: vec![],
        id: feature_id.clone(),
        layer_id: layer_id.clone(),
        geometry: FeatureGeometry::Point(GeoJsonPosition::new(-89.2, 13.7, None)),
        properties: BTreeMap::new(),
        semantic_type: "inspection_area".into(),
        time: None,
        feature_revision: 1,
        layer_revision: 1,
        schema_version: 1,
        deleted: false,
        title: None,
        related_resources: vec![],
        evidence_resources: vec![],
        provenance: FeatureProvenance {
            actor_id: veoveo_mcp_contract::PrincipalId::new("author").unwrap(),
            work_context: veoveo_mcp_contract::WorkContextId::new("operations").unwrap(),
            policy_revision: veoveo_mcp_contract::PolicyVersion::new("r1").unwrap(),
            invocation_mode: veoveo_mcp_contract::InvocationMode::Direct,
            initiator_id: None,
            delegation_id: None,
        },
        created_at: Utc::now(),
    };
    let mut draft = MapFeatureCommitDraft {
        identity,
        authority,
        layer_key: layer_id.to_string(),
        layer_canonical_json: "{}".into(),
        expected_layer_revision: 0,
        changeset_key: FeatureChangeSetId::new().to_string(),
        idempotency_key: "first".into(),
        request_digest_sha256: "b".repeat(64),
        changeset_canonical_json: "{}".into(),
        revisions: vec![MapFeatureRevisionDraft {
            feature_key: feature_id.to_string(),
            feature_revision: 1,
            layer_revision: 1,
            schema_version: 1,
            deleted: false,
            geometry_type: "Point".into(),
            geometry_json: serde_json::to_string(&feature.geometry).unwrap(),
            bbox_west: -89.2,
            bbox_south: 13.7,
            bbox_east: -89.2,
            bbox_north: 13.7,
            valid_from: None,
            valid_until: None,
            semantic_type: feature.semantic_type.clone(),
            title: None,
            canonical_json: serde_json::to_string(&feature).unwrap(),
            expected_feature_revision: None,
        }],
    };
    let first = store
        .commit_map_feature_changes(draft.clone())
        .await
        .unwrap()
        .changeset
        .commit_sequence;
    append_unrelated_events(&store).await;
    let snapshot = store.latest_outbox_sequence().await.unwrap();
    assert!(snapshot > first + 1_000);

    // Commit after the snapshot, then prove keyset paging honors both bounds.
    draft.expected_layer_revision = 1;
    draft.changeset_key = FeatureChangeSetId::new().to_string();
    draft.idempotency_key = "second".into();
    draft.request_digest_sha256 = "c".repeat(64);
    draft.revisions[0].expected_feature_revision = Some(1);
    draft.revisions[0].feature_revision = 2;
    draft.revisions[0].layer_revision = 2;
    let second_feature = MapFeature {
        feature_revision: 2,
        layer_revision: 2,
        ..feature
    };
    draft.revisions[0].canonical_json = serde_json::to_string(&second_feature).unwrap();
    let second = store
        .commit_map_feature_changes(draft.clone())
        .await
        .unwrap()
        .changeset
        .commit_sequence;
    let first_page = store.read_map_feature_commits(0, second, 1).await.unwrap();
    assert_eq!(first_page.len(), 1);
    assert_eq!(first_page[0].commit_sequence, first);
    assert_eq!(first_page[0].tenant_key, "map-recovery");
    assert!(
        store
            .read_map_feature_commits(first, snapshot, 1)
            .await
            .unwrap()
            .is_empty()
    );
    let second_page = store
        .read_map_feature_commits(first, second, 1)
        .await
        .unwrap();
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].commit_sequence, second);
    assert!(
        store
            .read_map_feature_commits(second, second, 1)
            .await
            .unwrap()
            .is_empty()
    );
    for (after, through, limit) in [
        (-1, second, 1),
        (second, first, 1),
        (0, second, 0),
        (0, second, 1001),
    ] {
        assert!(
            store
                .read_map_feature_commits(after, through, limit)
                .await
                .is_err()
        );
    }
    let mut plan = store.client().query(
        "SELECT tenant.slug AS tenant_key, work_context_key, layer_key, changeset_key, commit_sequence, resulting_layer_revision, feature_keys \
         FROM map_feature_changeset WHERE commit_sequence > $after AND commit_sequence <= $through ORDER BY commit_sequence ASC LIMIT 1 EXPLAIN;"
    ).bind(("after", first)).bind(("through", second)).await.unwrap().check().unwrap();
    let plan: Vec<serde_json::Value> = plan.take(0).unwrap();
    let plan = serde_json::to_string(&plan).unwrap();
    assert!(
        plan.contains("map_feature_changeset_commit_sequence"),
        "{plan}"
    );
    assert!(!plan.contains("TableScan"), "{plan}");
    println!("Map changeset recovery query plan: {plan}");

    let root = TempDir::new().unwrap();
    let config = MapAnalyticsConfig {
        database_path: root.path().join("map.duckdb"),
        authoring_task_root: root.path().join("tasks"),
        spill_dir: root.path().join("spill"),
        spatial_extension: extension.into(),
        memory_limit: "256MB".into(),
        threads: 1,
    };
    let projection =
        AuthoringProjection::new(store.clone(), MapAnalytics::open(config.clone()).unwrap());
    // Recreate an interrupted page using the same atomic writer as recovery.
    let revisions = projection
        .revisions_for_commit(&first_page[0])
        .await
        .unwrap();
    projection.apply_page(&revisions, first).unwrap();
    drop(projection);
    let projection =
        AuthoringProjection::new(store.clone(), MapAnalytics::open(config.clone()).unwrap());
    assert_eq!(projection.sequence().unwrap(), first as u64);
    assert_eq!(
        projection.reconcile_through(second as u64).await.unwrap(),
        second as u64
    );
    assert_projection_rows(&projection, 2, 2);
    assert_eq!(projection.reconcile().await.unwrap(), second as u64);
    assert_projection_rows(&projection, 2, 2);

    // Unrelated writers cannot move Map's committed recovery boundary.
    append_unrelated_events(&store).await;
    let through = store.latest_map_feature_commit_sequence().await.unwrap();
    assert_eq!(through, second);
    assert!(store.latest_outbox_sequence().await.unwrap() > through);
    assert_eq!(projection.reconcile().await.unwrap(), through as u64);
    assert_projection_rows(&projection, 2, 2);
    assert!(
        projection
            .reconcile_through(through as u64 + 1)
            .await
            .is_err()
    );
    assert!(projection.reconcile_through(u64::MAX).await.is_err());
    assert_eq!(projection.sequence().unwrap(), through as u64);
    drop(projection);
    let projection = AuthoringProjection::new(store.clone(), MapAnalytics::open(config).unwrap());
    assert_eq!(projection.reconcile().await.unwrap(), through as u64);
    assert_projection_rows(&projection, 2, 2);

    // Missing canonical data must fail closed without advancing the checkpoint.
    draft.expected_layer_revision = 2;
    draft.changeset_key = FeatureChangeSetId::new().to_string();
    draft.idempotency_key = "third".into();
    draft.request_digest_sha256 = "d".repeat(64);
    draft.revisions[0].expected_feature_revision = Some(2);
    draft.revisions[0].feature_revision = 3;
    draft.revisions[0].layer_revision = 3;
    let third_feature = MapFeature {
        feature_revision: 3,
        layer_revision: 3,
        ..second_feature
    };
    draft.revisions[0].canonical_json = serde_json::to_string(&third_feature).unwrap();
    let third = store.commit_map_feature_changes(draft).await.unwrap();
    store
        .client()
        .query("DELETE ONLY $revision;")
        .bind(("revision", third.revisions[0].id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(projection.reconcile().await.is_err());
    assert_eq!(projection.sequence().unwrap(), through as u64);
    assert_eq!(store.outbox_checkpoint(CONSUMER).await.unwrap(), through);
    assert_projection_rows(&projection, 2, 2);
    assert_late_sequence_rolls_back(&store, third.changeset.id).await;
}

async fn assert_late_sequence_rolls_back(
    store: &PlatformStore,
    source: veoveo_platform_store::RecordId,
) {
    // Allocation is independent of the enclosing transaction. Reserve the older
    // number, commit a newer changeset, then attempt the delayed older commit.
    let mut response = store
        .client()
        .query("RETURN sequence::nextval('platform_outbox_sequence');")
        .await
        .unwrap()
        .check()
        .unwrap();
    let delayed_sequence = response.take::<Option<i64>>(0).unwrap().unwrap();
    let query = "BEGIN TRANSACTION; \
        LET $copy = (SELECT * OMIT id FROM ONLY $source); \
        CREATE ONLY type::record('map_feature_changeset', ['map-recovery', 'sequence-probe', $key]) \
        CONTENT object::extend($copy, {changeset_key: $key, idempotency_key: $key, commit_sequence: $sequence}); \
        COMMIT TRANSACTION;";
    let mut response = store
        .client()
        .query("RETURN sequence::nextval('platform_outbox_sequence');")
        .await
        .unwrap()
        .check()
        .unwrap();
    let newer_sequence = response.take::<Option<i64>>(0).unwrap().unwrap();
    store
        .client()
        .query(query)
        .bind(("source", source.clone()))
        .bind(("key", "newer"))
        .bind(("sequence", newer_sequence))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        store.latest_map_feature_commit_sequence().await.unwrap(),
        newer_sequence
    );
    let mut rejected = store
        .client()
        .query(query)
        .bind(("source", source))
        .bind(("key", "delayed"))
        .bind(("sequence", delayed_sequence))
        .await
        .unwrap();
    let errors = rejected.take_errors();
    assert!(
        errors.values().any(|error| error
            .to_string()
            .contains("map_feature_projection_sequence_conflict")),
        "{errors:?}"
    );
    assert_eq!(
        store.latest_map_feature_commit_sequence().await.unwrap(),
        newer_sequence
    );
    let page = store
        .read_map_feature_commits(delayed_sequence - 1, newer_sequence, 10)
        .await
        .unwrap();
    assert_eq!(
        page.len(),
        1,
        "rejected changeset must roll back completely"
    );
    assert_eq!(page[0].commit_sequence, newer_sequence);
}

async fn append_unrelated_events(store: &PlatformStore) {
    let event = OutboxDraft::now(
        None,
        "test",
        "unrelated",
        "test.unrelated",
        1,
        OpenObject::default(),
    );
    store.client().query("BEGIN TRANSACTION; FOR $i IN 0..1001 { CREATE outbox_event CONTENT $event RETURN NONE; }; COMMIT TRANSACTION;")
        .bind(("event", event)).await.unwrap().check().unwrap();
}

fn assert_projection_rows(projection: &AuthoringProjection, revisions: i64, head_revision: i64) {
    let connection = projection.analytics.read_connection().unwrap();
    let count = connection
        .query_row(
            "SELECT count(*) FROM map_authored_feature_revision",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(count, revisions);
    let heads = connection
        .query_row(
            "SELECT count(*), max(feature_revision) FROM map_authored_feature_head",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .unwrap();
    assert_eq!(heads, (1, head_revision));
}
