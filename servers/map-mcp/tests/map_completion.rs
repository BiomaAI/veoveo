//! Native SQL completion bounds and catalog visibility.
use chrono::Utc;
use std::time::Duration;
use uuid::Uuid;
use veoveo_map_mcp::persistence::MapRepository;
use veoveo_map_mcp::persistence::{
    MapCatalogCompletion, MapReleaseDraft, MapReleaseState, MapRouteDraft, MapRouteMatrixDraft,
    MapRouteState, MapSourceDraft,
};
use veoveo_platform_store::*;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

fn key(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::now_v7())
}
async fn source(store: &PlatformStore, identity: &PlatformIdentity, key: String, dataset: &str) {
    MapRepository::new(store.clone())
        .create_map_source(MapSourceDraft {
            identity: identity.clone(),
            source_key: key,
            dataset_key: dataset.into(),
            name: "Fixture".into(),
            adapter_kind: "fixture".into(),
            authority_class: "reference".into(),
            map_families: vec!["land".into()],
            enabled: true,
            canonical_json: "{}".into(),
        })
        .await
        .unwrap();
}
async fn release(
    store: &PlatformStore,
    identity: &PlatformIdentity,
    dataset: &str,
    source: &str,
) -> String {
    let release = key("release");
    MapRepository::new(store.clone())
        .create_map_release(MapReleaseDraft {
            identity: identity.clone(),
            release_key: release.clone(),
            dataset_key: dataset.into(),
            source_key: source.into(),
            state: MapReleaseState::Staged,
            version_label: "fixture".into(),
            source_digest_sha256: "a".repeat(64),
            valid_from: Utc::now(),
            valid_until: None,
            canonical_json: "{}".into(),
        })
        .await
        .unwrap();
    release
}
async fn route_and_matrix(store: &PlatformStore, identity: &PlatformIdentity) -> (String, String) {
    let route = key("route");
    let matrix = key("matrix");
    let profile = key("mobility");
    let snapshot = key("snapshot");
    MapRepository::new(store.clone())
        .create_map_route(MapRouteDraft {
            identity: identity.clone(),
            route_key: route.clone(),
            status: MapRouteState::Validated,
            mobility_profile_key: profile.clone(),
            mobility_profile_version: 1,
            operational_snapshot_key: snapshot.clone(),
            departure_time: Utc::now(),
            arrival_time: None,
            cache_digest_sha256: "b".repeat(64),
            canonical_json: "{}".into(),
        })
        .await
        .unwrap();
    MapRepository::new(store.clone())
        .create_map_route_matrix(MapRouteMatrixDraft {
            identity: identity.clone(),
            matrix_key: matrix.clone(),
            mobility_profile_key: profile,
            mobility_profile_version: 1,
            operational_snapshot_key: snapshot,
            artifact_uri: None,
            canonical_json: Some("{}".into()),
        })
        .await
        .unwrap();
    (route, matrix)
}
#[tokio::test]
async fn catalog_completion_selects_matching_scoped_keys_before_dedup_and_limit() {
    tokio::time::timeout(Duration::from_secs(120), qualify())
        .await
        .expect("catalog completion exceeded 120 seconds");
}
async fn qualify() {
    let db = fixture::TestDb::with_modules(vec![
        veoveo_map_mcp::schema::module_setup(fixture::module_lanes::execution("map").unwrap())
            .unwrap(),
    ])
    .await;
    let identity =
        db.a.ensure_identity(
            "map-completion",
            "author",
            "https://fixture.local",
            "author",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let foreign =
        db.a.ensure_identity(
            "foreign",
            "author",
            "https://fixture.local",
            "author",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let peer =
        db.a.ensure_identity(
            "map-completion",
            "peer",
            "https://fixture.local",
            "peer",
            PrincipalKind::Service,
        )
        .await
        .unwrap();
    let dataset = key("dataset");
    let sources = [key("source")];
    source(&db.a, &identity, sources[0].clone(), &dataset).await;
    let first = release(&db.a, &identity, &dataset, &sources[0]).await;
    let second = release(&db.a, &identity, &dataset, &sources[0]).await;
    let other_dataset = key("dataset");
    let other = release(&db.a, &identity, &other_dataset, &sources[0]).await;
    release(&db.a, &foreign, &dataset, &sources[0]).await;
    assert_eq!(
        MapRepository::new(db.b.clone())
            .complete_map_catalog(
                &identity,
                MapCatalogCompletion::Release {
                    dataset: Some(dataset.clone())
                },
                ""
            )
            .await
            .unwrap(),
        vec![first, second]
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .complete_map_catalog(
                &identity,
                MapCatalogCompletion::Release {
                    dataset: Some(other_dataset.clone())
                },
                ""
            )
            .await
            .unwrap(),
        vec![other]
    );
    let mut datasets = vec![dataset, other_dataset];
    datasets.sort();
    assert_eq!(
        MapRepository::new(db.b.clone())
            .complete_map_catalog(&identity, MapCatalogCompletion::Dataset, "")
            .await
            .unwrap(),
        datasets
    );
    let (route, matrix) = route_and_matrix(&db.a, &identity).await;
    route_and_matrix(&db.a, &peer).await;
    route_and_matrix(&db.a, &foreign).await;
    assert_eq!(
        MapRepository::new(db.b.clone())
            .complete_map_catalog(&identity, MapCatalogCompletion::Route, "")
            .await
            .unwrap(),
        vec![route]
    );
    assert_eq!(
        MapRepository::new(db.b.clone())
            .complete_map_catalog(&identity, MapCatalogCompletion::Matrix, "")
            .await
            .unwrap(),
        vec![matrix]
    );
}
