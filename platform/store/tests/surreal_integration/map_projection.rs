use veoveo_platform_store::PlatformStore;

pub(super) async fn current_catalog_replays_commits(
    store: &PlatformStore,
    committed_sequence: i64,
) {
    assert!(store.schema_status().await.unwrap().is_current());
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
