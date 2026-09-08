use veoveo_platform_store::PlatformStore;

pub(super) async fn upgrade_populated_catalog(store: &PlatformStore, committed_sequence: i64) {
    // Recreate the pre-0047 schema in this isolated fixture while retaining its
    // real committed changeset, revisions, and outbox event.
    store.client().query(
        "REMOVE EVENT map_feature_changeset_projection_sequence ON TABLE map_feature_changeset; \
         REMOVE INDEX map_feature_changeset_commit_sequence ON TABLE map_feature_changeset; \
         REMOVE TABLE map_projection_state; \
         DELETE platform_schema_migration:47; DELETE platform_schema_migration:48;"
    ).await.unwrap().check().unwrap();
    let migration = store.migrate().await.unwrap();
    assert_eq!(migration.applied_versions, [47, 48]);
    assert_eq!(
        store.latest_map_feature_commit_sequence().await.unwrap(),
        committed_sequence
    );
    let commits = store
        .read_map_feature_commits(0, committed_sequence, 10)
        .await
        .unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].commit_sequence, committed_sequence);
    assert!(store.migrate().await.unwrap().applied_versions.is_empty());
    assert_eq!(
        store.latest_map_feature_commit_sequence().await.unwrap(),
        committed_sequence
    );
}
