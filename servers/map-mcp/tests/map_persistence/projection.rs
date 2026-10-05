use super::fixture;
use veoveo_map_mcp::persistence::MapRepository;

pub(super) async fn current_catalog_replays_commits(
    store: &MapRepository,
    committed_sequence: i64,
) {
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
    let registry = fixture::module_lanes::registry(vec![
        veoveo_map_mcp::schema::module_setup(fixture::module_lanes::execution("map").unwrap())
            .unwrap(),
    ])
    .unwrap();
    let prepared = veoveo_modules::runner::prepare(
        registry
            .select(vec![veoveo_modules::ModuleName::new("map").unwrap()])
            .unwrap(),
    )
    .unwrap();
    assert!(
        prepared
            .status(store.platform().client())
            .await
            .unwrap()
            .is_current()
    );
    prepared.apply(store.platform().client()).await.unwrap();
    assert_eq!(
        store.latest_map_feature_commit_sequence().await.unwrap(),
        committed_sequence
    );
}
