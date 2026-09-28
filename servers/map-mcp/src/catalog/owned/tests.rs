//! Native Store qualification; the shared fixture owns its container and rows.
use super::*;
use crate::contract::*;
use std::{collections::BTreeSet, time::Duration};
use veoveo_platform_store::{
    MapCatalogCompletion, MapRouteMatrixDraft, PlatformStore, PrincipalKind,
};

fn key(prefix: &str, n: usize) -> String {
    format!("{prefix}-{n:08x}-0000-7000-8000-000000000000")
}

async fn scope(store: &PlatformStore, tenant: &str, principal: &str) -> MapAccessContext {
    MapAccessContext {
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
    }
}

fn plan(n: usize, release: usize) -> RoutePlan {
    let now = Utc::now();
    RoutePlan {
        route_id: key("route", n).parse().unwrap(),
        route_uri: uris::route_uri(&key("route", n)),
        status: RouteStatus::Unavailable,
        mobility_profile_id: key("mobility", 1).parse().unwrap(),
        mobility_profile_version: crate::contract::MobilityProfileVersion::FIRST,
        departure_time: now,
        arrival_time: None,
        legs: vec![],
        alternatives: vec![],
        summary: RouteCost {
            distance: Meters::new(0.).unwrap(),
            duration: Seconds::new(0.).unwrap(),
            energy: None,
            fuel: None,
            monetary_minor_units: None,
            risk: Ratio::new(0.).unwrap(),
        },
        crossed_boundary_ids: BTreeSet::new(),
        facility_ids: BTreeSet::new(),
        restriction_ids: BTreeSet::new(),
        validation_id: key("validation", n).parse().unwrap(),
        provenance: RouteProvenance {
            base_release_ids: BTreeSet::from([key("release", release).parse().unwrap()]),
            operational_snapshot_id: key("snapshot", 1).parse().unwrap(),
            planner_version: "fixture".into(),
            cost_model_version: "fixture".into(),
        },
        created_at: now,
    }
}

async fn acquisition(catalog: &MapCatalog, scope: &MapAccessContext, n: usize) -> AcquisitionJob {
    catalog
        .create_acquisition(
            scope,
            CreateAcquisitionRequest {
                source_id: key("source", 1).parse().unwrap(),
                requested_coverage: Wgs84BoundingBox {
                    west: -1.,
                    south: -1.,
                    east: 1.,
                    north: 1.,
                },
                expected_source_digest_sha256: None,
                idempotency_key: format!("acquisition-{n}"),
            },
            key("acquisition", n).parse().unwrap(),
        )
        .await
        .unwrap()
}

async fn records(catalog: &MapCatalog, scope: &MapAccessContext, n: usize) {
    let plan = plan(n, 1);
    catalog
        .persist_route(scope, &plan, "a".repeat(64))
        .await
        .unwrap();
    catalog
        .persist_matrix(
            scope,
            &RouteMatrix {
                matrix_id: key("matrix", n).parse().unwrap(),
                cells: vec![RouteMatrixCell {
                    origin_index: 0,
                    destination_index: 0,
                    status: RouteStatus::Unavailable,
                    cost: None,
                }],
                provenance: plan.provenance,
                created_at: plan.created_at,
            },
            &key("mobility", 1).parse().unwrap(),
            MobilityProfileVersion::FIRST,
        )
        .await
        .unwrap();
    acquisition(catalog, scope, n).await;
}

#[tokio::test]
async fn owned_pages_direct_reads_recovery_and_invalidation_select_in_sql() {
    tokio::time::timeout(Duration::from_secs(120), qualify())
        .await
        .expect("owned Map qualification exceeded 120 seconds");
}

async fn qualify() {
    let db = crate::test_store::TestDb::new().await;
    let writer = MapCatalog::new(db.a.clone());
    let reader = MapCatalog::new(db.b.clone());
    let owner = scope(&db.a, "map-owned", "author").await;
    let peer = scope(&db.a, "map-owned", "peer").await;
    let foreign = scope(&db.a, "foreign", "author").await;
    for n in 0..110 {
        records(&writer, &foreign, n).await;
    }
    for n in 110..220 {
        records(&writer, &peer, n).await;
    }
    for n in 1000..1125 {
        records(&writer, &owner, n).await;
    }
    // Artifact-only storage rows have no readable matrix document. They cannot
    // consume a resource page or completion slot.
    for n in 500..610 {
        db.a.create_map_route_matrix(MapRouteMatrixDraft {
            identity: owner.identity.clone(),
            matrix_key: key("matrix", n),
            mobility_profile_key: key("mobility", 1),
            mobility_profile_version: 1,
            operational_snapshot_key: key("snapshot", 1),
            artifact_uri: Some("artifact://fixture".into()),
            canonical_json: None,
        })
        .await
        .unwrap();
    }
    for collection in [
        Collection::Routes,
        Collection::Matrices,
        Collection::Acquisitions,
    ] {
        let mut after = None;
        let mut ids = Vec::new();
        let mut lengths = Vec::new();
        for _ in 0..3 {
            let (keys, cursor) = match collection {
                Collection::Routes => {
                    let page = reader.routes_page(&owner, after.as_deref()).await.unwrap();
                    assert_eq!(page.limit, 100);
                    let wire = serde_json::to_value(&page).unwrap();
                    assert!(wire["items"][0].get("legs").is_none());
                    assert_eq!(
                        wire["items"][0]["resource_uri"],
                        uris::route_uri(&page.items[0].route_id.to_string())
                    );
                    (
                        page.items
                            .into_iter()
                            .map(|item| item.route_id.to_string())
                            .collect::<Vec<_>>(),
                        page.next_cursor,
                    )
                }
                Collection::Matrices => {
                    let page = reader
                        .matrices_page(&owner, after.as_deref())
                        .await
                        .unwrap();
                    assert_eq!(page.limit, 100);
                    assert!(
                        serde_json::to_value(&page).unwrap()["items"][0]
                            .get("cells")
                            .is_none()
                    );
                    (
                        page.items
                            .into_iter()
                            .map(|item| item.matrix_id.to_string())
                            .collect::<Vec<_>>(),
                        page.next_cursor,
                    )
                }
                Collection::Acquisitions => {
                    let page = reader
                        .acquisitions_page(&owner, after.as_deref())
                        .await
                        .unwrap();
                    assert_eq!(page.limit, 100);
                    (
                        page.items
                            .into_iter()
                            .map(|item| item.acquisition_id.to_string())
                            .collect::<Vec<_>>(),
                        page.next_cursor,
                    )
                }
            };
            lengths.push(keys.len());
            ids.extend(keys);
            after = collection.parse_cursor(cursor.as_deref()).unwrap();
            if after.is_none() {
                break;
            }
        }
        assert!(after.is_none());
        assert_eq!(lengths, [100, 25]);
        let prefix = match collection {
            Collection::Routes => "route",
            Collection::Matrices => "matrix",
            Collection::Acquisitions => "acquisition",
        };
        assert_eq!(
            ids,
            (1000..1125).map(|n| key(prefix, n)).collect::<Vec<_>>()
        );
    }
    let route = key("route", 1124).parse().unwrap();
    let matrix = key("matrix", 1124).parse().unwrap();
    let job = key("acquisition", 1124).parse().unwrap();
    assert!(reader.route(&owner, &route).await.unwrap().is_some());
    assert!(reader.matrix(&owner, &matrix).await.unwrap().is_some());
    assert!(reader.acquisition(&owner, &job).await.unwrap().is_some());
    for denied in [&peer, &foreign] {
        assert!(reader.route(denied, &route).await.unwrap().is_none());
        assert!(reader.matrix(denied, &matrix).await.unwrap().is_none());
        assert!(reader.acquisition(denied, &job).await.unwrap().is_none());
        let owned_job = reader.acquisition(&owner, &job).await.unwrap().unwrap();
        assert!(writer.update_acquisition(denied, owned_job).await.is_err());
    }
    assert_eq!(
        reader
            .acquisition(&owner, &job)
            .await
            .unwrap()
            .unwrap()
            .record_version,
        1
    );
    assert!(
        reader
            .matrix(&owner, &key("matrix", 500).parse().unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let completions =
        db.b.complete_map_catalog(&owner.identity, MapCatalogCompletion::Matrix, "")
            .await
            .unwrap();
    assert_eq!(
        completions,
        (1000..1101).map(|n| key("matrix", n)).collect::<Vec<_>>()
    );
    for limit in [0, 102] {
        assert!(
            db.b.map_routes_page(&owner.identity, None, limit)
                .await
                .is_err()
        );
        assert!(
            db.b.map_matrices_page(&owner.identity, None, limit)
                .await
                .is_err()
        );
        assert!(
            db.b.map_acquisitions_page(&owner.identity, None, limit)
                .await
                .is_err()
        );
    }
    assert!(
        db.b.map_routes_page(&owner.identity, Some("route-invalid"), 1)
            .await
            .is_err()
    );
    qualify_recovery(&writer, &reader, &owner, &peer).await;
    qualify_invalidation(&writer, &reader, &owner, &peer, &foreign).await;
}

async fn qualify_recovery(
    writer: &MapCatalog,
    reader: &MapCatalog,
    owner: &MapAccessContext,
    peer: &MapAccessContext,
) {
    for n in 1000..1060 {
        let mut job = reader
            .acquisition(owner, &key("acquisition", n).parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        job.status = match n % 3 {
            1 => AcquisitionStatus::Succeeded,
            2 => AcquisitionStatus::Failed,
            _ => AcquisitionStatus::Cancelled,
        };
        writer.update_acquisition(owner, job).await.unwrap();
    }
    let active = (1060..1110)
        .map(|n| key("acquisition", n).parse().unwrap())
        .collect::<Vec<_>>();
    for n in 2000..2125 {
        acquisition(writer, owner, n).await;
    }
    for (n, status) in [
        (2000, AcquisitionStatus::Running),
        (2001, AcquisitionStatus::CancelRequested),
    ] {
        let mut job = reader
            .acquisition(owner, &key("acquisition", n).parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        job.status = status;
        writer.update_acquisition(owner, job).await.unwrap();
    }
    let mut after = None;
    let mut recovered = Vec::new();
    let mut lengths = Vec::new();
    loop {
        let jobs = reader
            .interrupted_acquisitions_batch(owner, &active, after.as_ref())
            .await
            .unwrap();
        if jobs.is_empty() {
            break;
        }
        lengths.push(jobs.len());
        after = jobs.last().map(|job| job.acquisition_id.clone());
        recovered.extend(jobs.into_iter().map(|job| job.acquisition_id.to_string()));
    }
    writer
        .reconcile_interrupted_acquisitions(owner, &active)
        .await
        .unwrap();
    assert!(
        reader
            .interrupted_acquisitions_batch(owner, &active, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(lengths, [100, 40]);
    assert_eq!(
        recovered,
        (1110..1125)
            .chain(2000..2125)
            .map(|n| key("acquisition", n))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        reader
            .acquisition(owner, &active[0])
            .await
            .unwrap()
            .unwrap()
            .status,
        AcquisitionStatus::Queued
    );
    assert_eq!(
        reader
            .acquisition(owner, &key("acquisition", 1000).parse().unwrap())
            .await
            .unwrap()
            .unwrap()
            .status,
        AcquisitionStatus::Succeeded
    );
    assert_eq!(
        reader
            .acquisition(peer, &key("acquisition", 110).parse().unwrap())
            .await
            .unwrap()
            .unwrap()
            .status,
        AcquisitionStatus::Queued
    );
}

async fn qualify_invalidation(
    writer: &MapCatalog,
    reader: &MapCatalog,
    owner: &MapAccessContext,
    peer: &MapAccessContext,
    foreign: &MapAccessContext,
) {
    let unaffected = plan(4000, 2);
    writer
        .persist_route(owner, &unaffected, "b".repeat(64))
        .await
        .unwrap();
    writer
        .store()
        .client()
        .query("DELETE map_route_dependency WHERE tenant = $tenant AND route_key = $route;")
        .bind(("tenant", owner.identity.tenant_id.record_id()))
        .bind(("route", key("route", 1124)))
        .await
        .unwrap()
        .check()
        .unwrap();
    writer
        .store()
        .create_map_route_dependency(veoveo_platform_store::MapRouteDependencyDraft {
            tenant_id: owner.identity.tenant_id,
            route_key: unaffected.route_id.to_string(),
            dependency_kind: MapDependencyKind::Release,
            dependency_key: key("release", 1),
        })
        .await
        .unwrap();
    let restriction = key("restriction", 1).parse().unwrap();
    let facility = key("facility", 1).parse().unwrap();
    let mut restricted = plan(4001, 2);
    restricted.restriction_ids.insert(restriction);
    restricted.facility_ids.insert(facility);
    writer
        .persist_route(owner, &restricted, "c".repeat(64))
        .await
        .unwrap();
    let facility_rows = writer
        .store()
        .map_routes_for_dependency_page(
            owner.identity.tenant_id,
            MapDependencyKind::Facility,
            &key("facility", 1),
            None,
            100,
        )
        .await
        .unwrap();
    assert_eq!(facility_rows.len(), 1);
    assert_eq!(facility_rows[0].route_key, restricted.route_id.as_str());
    let release = key("release", 1).parse().unwrap();
    // Domain administration reaches every affected tenant owner, in three SQL batches.
    assert_eq!(
        writer
            .invalidate_routes_for_release(owner, &release)
            .await
            .unwrap(),
        235
    );
    assert_eq!(
        writer
            .invalidate_routes_for_release(owner, &release)
            .await
            .unwrap(),
        0
    );
    for (scope, n) in [(owner, 1124), (peer, 110)] {
        let id = key("route", n).parse().unwrap();
        assert_eq!(
            reader.route(scope, &id).await.unwrap().unwrap().status,
            RouteStatus::Invalidated
        );
        assert_eq!(
            reader
                .store()
                .map_route(&scope.identity, id.as_str())
                .await
                .unwrap()
                .unwrap()
                .status,
            MapRouteState::Invalidated
        );
    }
    assert_eq!(
        reader
            .route(owner, &unaffected.route_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        RouteStatus::Unavailable
    );
    assert_eq!(
        reader
            .route(foreign, &key("route", 0).parse().unwrap())
            .await
            .unwrap()
            .unwrap()
            .status,
        RouteStatus::Unavailable
    );
    assert_eq!(
        writer
            .invalidate_routes_for_restriction(owner, &key("restriction", 1).parse().unwrap())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        reader
            .route(owner, &restricted.route_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        RouteStatus::Invalidated
    );
    assert!(
        !writer
            .store()
            .invalidate_map_route(owner.identity.tenant_id, &key("route", 1124), "{}".into())
            .await
            .unwrap()
    );
    assert!(
        !writer
            .store()
            .invalidate_map_route(foreign.identity.tenant_id, &key("route", 4000), "{}".into())
            .await
            .unwrap()
    );
}

#[test]
fn cursors_reject_cross_collection_keys_versions_and_unbounded_input() {
    let encode = |version, collection, after: String| {
        hex::encode(
            serde_json::to_vec(&Cursor {
                version,
                collection,
                after,
            })
            .unwrap(),
        )
    };
    let cursor = encode(1, Collection::Routes, key("route", 100));
    assert_eq!(
        Collection::Routes.parse_cursor(Some(&cursor)).unwrap(),
        Some(key("route", 100))
    );
    assert!(Collection::Matrices.parse_cursor(Some(&cursor)).is_err());
    assert!(
        Collection::Acquisitions
            .parse_cursor(Some(&cursor))
            .is_err()
    );
    for invalid in [
        "".into(),
        "gg".into(),
        "a".repeat(2049),
        encode(2, Collection::Routes, key("route", 100)),
        encode(1, Collection::Routes, key("matrix", 100)),
    ] {
        assert!(Collection::Routes.parse_cursor(Some(&invalid)).is_err());
    }
}
