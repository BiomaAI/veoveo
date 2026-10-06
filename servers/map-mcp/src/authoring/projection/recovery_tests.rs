use crate::persistence::MapRepository;
use std::collections::BTreeMap;

use crate::persistence::{
    MapFeatureCommitDraft, MapFeatureLayerDraft, MapFeatureRevisionDraft, MapFeatureSchemaDraft,
};
use tempfile::TempDir;
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, InvocationAuthorityRecord, InvocationMode, PrincipalKind,
    WorkContextMembershipLevel,
};

use crate::{analytics::MapAnalyticsConfig, contract::*};

use super::*;

#[tokio::test]
async fn recovery_pages_map_commits_and_resumes_the_persisted_projection() {
    tokio::time::timeout(std::time::Duration::from_secs(180), recovery())
        .await
        .expect("Map projection recovery exceeded 180 seconds");
}

async fn recovery() {
    let extension = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION").unwrap();
    let db = crate::test_store::TestDb::with_modules(vec![
        crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
            .unwrap(),
    ])
    .await;
    let store = db.a.clone();
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
    let now = Utc::now();
    let context = veoveo_platform_store::WorkContextRecord {
        id: veoveo_platform_store::deterministic_work_context_id("map-recovery", "operations")
            .unwrap()
            .record_id(),
        tenant: identity.tenant_id.record_id(),
        context_key: "operations".into(),
        title: "Recovery".into(),
        policy_revision: authority.policy_revision.clone(),
        output_policy: veoveo_platform_store::WorkContextOutputPolicyRecord {
            owner_kind: authority.owner_kind,
            owner_key: authority.owner_key.clone(),
            initial_grants: vec![],
            classification: None,
            data_labels: vec![],
        },
        memberships: vec![],
        created_at: now,
        updated_at: now,
    };
    store
        .client()
        .query(include_str!(
            "../../queries/authoring/projection/recovery_tests/recovery/create_context.surql"
        ))
        .bind(("context", context))
        .await
        .unwrap()
        .check()
        .unwrap();
    let layer_id = FeatureLayerId::new();
    let feature_id = MapFeatureId::new();
    MapRepository::new(store.clone())
        .create_map_feature_layer(MapFeatureLayerDraft {
            identity: identity.clone(),
            authority: authority.clone(),
            layer_key: crate::contract::FeatureLayerId::parse(layer_id.clone()).unwrap(),
            title: "Recovery".into(),
            description: None,
            content_class: "boundaries".into(),
            schema: MapFeatureSchemaDraft {
                schema_revision_key: FeatureSchemaRevisionId::new(),
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
    let feature = MapFeature::new(crate::contract::MapFeatureValue {
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
            actor_id: veoveo_types::PrincipalId::parse("author").unwrap(),
            work_context: veoveo_types::WorkContextId::parse("operations").unwrap(),
            policy_revision: veoveo_types::PolicyVersion::parse("r1").unwrap(),
            invocation_mode: veoveo_types::InvocationMode::Direct,
            initiator_id: None,
            delegation_id: None,
        },
        created_at: Utc::now(),
    })
    .expect("admitted Map fixture");
    let mut draft = MapFeatureCommitDraft {
        identity,
        authority,
        layer_key: crate::contract::FeatureLayerId::parse(layer_id.clone()).unwrap(),
        layer_canonical_json: "{}".into(),
        expected_layer_revision: 0,
        changeset_key: FeatureChangeSetId::new().clone(),
        idempotency_key: "first".into(),
        request_digest_sha256: "b".repeat(64),
        changeset_canonical_json: "{}".into(),
        revisions: vec![MapFeatureRevisionDraft {
            feature_key: crate::contract::MapFeatureId::parse(feature_id.clone()).unwrap(),
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
    let first = MapRepository::new(store.clone())
        .commit_map_feature_changes(draft.clone())
        .await
        .unwrap()
        .changeset
        .commit_sequence;
    append_unrelated_changes(&store).await;
    let snapshot = MapRepository::new(store.clone())
        .latest_map_feature_commit_sequence()
        .await
        .unwrap();
    assert_eq!(snapshot, first);

    // Commit after the snapshot, then prove keyset paging honors both bounds.
    draft.expected_layer_revision = 1;
    draft.changeset_key = FeatureChangeSetId::new();
    draft.idempotency_key = "second".into();
    draft.request_digest_sha256 = "c".repeat(64);
    draft.revisions[0].expected_feature_revision = Some(1);
    draft.revisions[0].feature_revision = 2;
    draft.revisions[0].layer_revision = 2;
    let second_feature = MapFeature::new(crate::contract::MapFeatureValue {
        feature_revision: 2,
        layer_revision: 2,
        ..feature.clone().into_value()
    })
    .expect("admitted Map fixture");
    draft.revisions[0].canonical_json = serde_json::to_string(&second_feature).unwrap();
    let second = MapRepository::new(store.clone())
        .commit_map_feature_changes(draft.clone())
        .await
        .unwrap()
        .changeset
        .commit_sequence;
    let first_page = MapRepository::new(store.clone())
        .read_map_feature_commits(0, second, 1)
        .await
        .unwrap();
    assert_eq!(first_page.len(), 1);
    assert_eq!(first_page[0].commit_sequence, first);
    assert_eq!(first_page[0].tenant_key, "map-recovery");
    assert!(
        MapRepository::new(store.clone())
            .read_map_feature_commits(first, snapshot, 1)
            .await
            .unwrap()
            .is_empty()
    );
    let second_page = MapRepository::new(store.clone())
        .read_map_feature_commits(first, second, 1)
        .await
        .unwrap();
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].commit_sequence, second);
    assert!(
        MapRepository::new(store.clone())
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
            MapRepository::new(store.clone())
                .read_map_feature_commits(after, through, limit)
                .await
                .is_err()
        );
    }
    let mut plan = store
        .client()
        .query(include_str!(
            "../../queries/authoring/projection/recovery_tests/recovery/statement_1.surql"
        ))
        .bind(("after", first))
        .bind(("through", second))
        .await
        .unwrap()
        .check()
        .unwrap();
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
        AuthoringProjection::new(db.b.clone(), MapAnalytics::open(config.clone()).unwrap());
    // Recreate an interrupted page using the same atomic writer as recovery.
    let revisions = projection
        .revisions_for_commit(&first_page[0])
        .await
        .unwrap();
    projection.apply_page(&revisions, first).unwrap();
    drop(projection);
    let projection =
        AuthoringProjection::new(db.b.clone(), MapAnalytics::open(config.clone()).unwrap());
    assert_eq!(projection.sequence().unwrap(), first as u64);
    let tenant = draft.identity.tenant_id.record_id();
    let context =
        veoveo_platform_store::deterministic_work_context_id("map-recovery", "operations")
            .unwrap()
            .record_id();
    let foreign = store
        .ensure_identity(
            "map-recovery-foreign",
            "author",
            "https://veoveo.local/services",
            "author",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    store
        .client()
        .query(include_str!(
            "../../queries/authoring/projection/recovery_tests/recovery/identity_metadata.surql"
        ))
        .bind(("context", context.clone()))
        .bind(("tenant", foreign.tenant_id.record_id()))
        .bind(("enabled", true))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        projection.reconcile_through(second as u64).await.is_err(),
        "corrupt context association must stop recovery before advancing the checkpoint"
    );
    assert_eq!(projection.sequence().unwrap(), first as u64);
    store
        .client()
        .query(include_str!(
            "../../queries/authoring/projection/recovery_tests/recovery/identity_metadata.surql"
        ))
        .bind(("context", context.clone()))
        .bind(("tenant", tenant.clone()))
        .bind(("enabled", false))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        MapRepository::new(store.clone())
            .read_map_feature_commits(first, second, 1)
            .await
            .unwrap()[0]
            .tenant_key,
        "map-recovery",
        "disabled tenant metadata remains available for retained journal recovery"
    );

    assert_eq!(
        projection.reconcile_through(second as u64).await.unwrap(),
        second as u64
    );
    assert_projection_rows(&projection, 2, 2);
    store
        .client()
        .query(include_str!(
            "../../queries/authoring/projection/recovery_tests/recovery/identity_metadata.surql"
        ))
        .bind(("context", context))
        .bind(("tenant", tenant))
        .bind(("enabled", true))
        .await
        .unwrap()
        .check()
        .unwrap();

    assert_eq!(projection.reconcile().await.unwrap(), second as u64);
    assert_projection_rows(&projection, 2, 2);

    // Unrelated writers cannot move Map's committed recovery boundary.
    append_unrelated_changes(&store).await;
    let through = MapRepository::new(store.clone())
        .latest_map_feature_commit_sequence()
        .await
        .unwrap();
    assert_eq!(through, second);
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
    let projection = AuthoringProjection::new(db.b.clone(), MapAnalytics::open(config).unwrap());
    assert_eq!(projection.reconcile().await.unwrap(), through as u64);
    assert_projection_rows(&projection, 2, 2);

    // Missing canonical data must fail closed without advancing the checkpoint.
    draft.expected_layer_revision = 2;
    draft.changeset_key = FeatureChangeSetId::new();
    draft.idempotency_key = "third".into();
    draft.request_digest_sha256 = "d".repeat(64);
    draft.revisions[0].expected_feature_revision = Some(2);
    draft.revisions[0].feature_revision = 3;
    draft.revisions[0].layer_revision = 3;
    let third_feature = MapFeature::new(crate::contract::MapFeatureValue {
        feature_revision: 3,
        layer_revision: 3,
        ..second_feature.clone().into_value()
    })
    .expect("admitted Map fixture");
    draft.revisions[0].canonical_json = serde_json::to_string(&third_feature).unwrap();
    let third = MapRepository::new(store.clone())
        .commit_map_feature_changes(draft)
        .await
        .unwrap();
    store
        .client()
        .query(include_str!(
            "../../queries/authoring/projection/recovery_tests/recovery/statement_2.surql"
        ))
        .bind(("revision", third.revisions[0].id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(projection.reconcile().await.is_err());
    assert_eq!(projection.sequence().unwrap(), through as u64);
    assert_projection_rows(&projection, 2, 2);
    assert_late_sequence_rolls_back(&store, third.changeset.id).await;
}

async fn assert_late_sequence_rolls_back(
    store: &PlatformStore,
    source: veoveo_platform_store::RecordId,
) {
    // The database rejects a stale sequence even for a direct trusted write.
    let delayed_sequence = MapRepository::new(store.clone())
        .latest_map_feature_commit_sequence()
        .await
        .unwrap()
        + 1;
    let query = include_str!(
        "../../queries/authoring/projection/recovery_tests/assert_late_sequence_rolls_back/statement_1.surql"
    );
    let newer_sequence = delayed_sequence + 1;
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
        MapRepository::new(store.clone())
            .latest_map_feature_commit_sequence()
            .await
            .unwrap(),
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
        MapRepository::new(store.clone())
            .latest_map_feature_commit_sequence()
            .await
            .unwrap(),
        newer_sequence
    );
    let page = MapRepository::new(store.clone())
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

async fn append_unrelated_changes(store: &PlatformStore) {
    store.client().query(include_str!("../../queries/authoring/projection/recovery_tests/append_unrelated_changes/statement_1.surql"))
        .bind(("batch", uuid::Uuid::now_v7().to_string()))
        .await.unwrap().check().unwrap();
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
