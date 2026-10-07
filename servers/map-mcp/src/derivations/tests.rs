use std::{sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{ResourceUpdate, SubscriptionHub};
use veoveo_platform_store::PrincipalKind;

use super::*;
use crate::{contract::*, test_store::TestDb};

async fn map_scope(store: &veoveo_platform_store::PlatformStore, tenant: &str) -> MapAccessContext {
    MapAccessContext {
        identity: store
            .ensure_identity(
                tenant,
                "author",
                "https://fixture.local/services",
                "author",
                PrincipalKind::Service,
            )
            .await
            .unwrap(),
    }
}
fn context() -> WorkContextId {
    WorkContextId::parse("operations").unwrap()
}
fn raster(n: usize) -> RasterDerivation {
    RasterDerivation::new(crate::contract::RasterDerivationValue {
        schema_version: RASTER_DERIVATION_SCHEMA_VERSION,
        derivation_id: RasterDerivationId::parse(format!(
            "raster-derivation-{n:08x}-0000-7000-8000-000000000000"
        ))
        .unwrap(),
        source_raster_id: RasterProductId::new(),
        source_release_id: DatasetReleaseId::new(),
        source_checksum_sha256: "a".repeat(64),
        source_crs: "EPSG:4326".into(),
        source_transform: [0., 1., 0., 0., 0., -1.],
        operation: RasterDerivationOperation::Sample {
            band: 1,
            positions: vec![Wgs84Position::new(-89., 13., None).unwrap()],
        },
        algorithm_revision: RASTER_DERIVATION_ALGORITHM_REVISION.into(),
        output_artifact_uri: veoveo_artifact_contract::ArtifactId::new().plane_uri(),
        output_mime_type: "application/json".into(),
        output_crs: "EPSG:4326".into(),
        output_transform: None,
        output_checksum_sha256: "b".repeat(64),
        created_by: PrincipalId::parse("author").unwrap(),
        work_context: context(),
        created_at: Utc::now(),
    })
    .expect("admitted Map fixture")
}
fn spatial() -> SpatialDerivation {
    let id = SpatialDerivationId::new();
    SpatialDerivation::new(crate::contract::SpatialDerivationValue {
        schema_version: SPATIAL_DERIVATION_SCHEMA_VERSION,
        resource_uri: crate::contract::MapSpatialDerivationUri::new(id.clone()),
        derivation_id: id,
        operation: SpatialDerivationOperation::ValidateRoute {
            route: Wgs84LineString {
                coordinates: vec![
                    Wgs84Position::new(-89., 13., None).unwrap(),
                    Wgs84Position::new(-89.01, 13., None).unwrap(),
                ],
            },
        },
        geometries: vec![SpatialGeometry {
            role: SpatialGeometryRole::ValidatedRoute,
            ordinal: 0,
            geometry: FeatureGeometry::LineString(vec![
                GeoJsonPosition::new(-89., 13., None),
                GeoJsonPosition::new(-89.01, 13., None),
            ]),
        }],
        ordered_input_ids: vec![],
        connected_components: vec![],
        valid: true,
        findings: vec![],
        mobility_profile_id: MobilityProfileId::new(),
        mobility_profile_version: crate::contract::MobilityProfileVersion::FIRST,
        source_release_ids: Default::default(),
        intersected_restriction_ids: Default::default(),
        terrain_classes: Default::default(),
        effective_at: Utc::now(),
        projection: SpatialProjection {
            profile: "fixture".into(),
            origin: Wgs84Position::new(-89., 13., None).unwrap(),
            earth_radius_m: 6_371_008.8,
        },
        algorithm_revision: SPATIAL_DERIVATION_ALGORITHM_REVISION.into(),
        request_digest_sha256: "c".repeat(64),
        geometry_digest_sha256: "d".repeat(64),
        created_by: PrincipalId::parse("author").unwrap(),
        work_context: context(),
        created_at: Utc::now(),
    })
    .expect("admitted Map fixture")
}

#[test]
fn derivation_roots_admit_current_wire_and_refuse_retired_forms() {
    fn qualify<T: serde::Serialize + serde::de::DeserializeOwned + schemars::JsonSchema>(value: T) {
        let current = serde_json::to_value(value).unwrap();
        let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&current));
        serde_json::from_value::<T>(current.clone()).unwrap_or_else(|error| panic!("{error}"));
        for version in [1, 3] {
            let mut bad = current.clone();
            bad["schemaVersion"] = serde_json::json!(version);
            assert!(!validator.is_valid(&bad));
            assert!(serde_json::from_value::<T>(bad).is_err());
        }
        for (canonical, retired) in [
            ("schemaVersion", "schema_version"),
            ("derivationId", "derivation_id"),
            ("createdBy", "created_by"),
            ("workContext", "work_context"),
        ] {
            for mixed in [false, true] {
                let mut bad = current.clone();
                let value = bad[canonical].clone();
                assert!(!value.is_null());
                if !mixed {
                    bad.as_object_mut().unwrap().remove(canonical);
                }
                bad[retired] = value;
                assert!(!validator.is_valid(&bad));
                assert!(serde_json::from_value::<T>(bad).is_err());
            }
        }
    }
    qualify(raster(1));
    qualify(spatial());
}

#[test]
fn derivation_builders_and_decoders_share_value_and_uri_admission() {
    let value = raster(1);
    assert_eq!(
        serde_json::from_value::<RasterDerivation>(serde_json::to_value(&value).unwrap()).unwrap(),
        value
    );
    let mut bad = value.into_value();
    bad.source_checksum_sha256 = "bad".into();
    assert!(RasterDerivation::new(bad.clone()).is_err());
    assert!(
        serde_json::from_value::<RasterDerivation>(serde_json::to_value(bad).unwrap()).is_err()
    );
    let value = spatial();
    assert_eq!(
        serde_json::from_value::<SpatialDerivation>(serde_json::to_value(&value).unwrap()).unwrap(),
        value
    );
    let mut bad = value.into_value();
    bad.resource_uri = MapSpatialDerivationUri::new(SpatialDerivationId::new());
    assert!(SpatialDerivation::new(bad.clone()).is_err());
    assert!(
        serde_json::from_value::<SpatialDerivation>(serde_json::to_value(bad).unwrap()).is_err()
    );
}

#[tokio::test]
async fn sql_pages_completion_and_immutable_reads_cross_replicas() {
    tokio::time::timeout(Duration::from_secs(180), qualify_sql_pages())
        .await
        .expect("Map SQL qualification exceeded 180 seconds");
}

async fn qualify_sql_pages() {
    let db = TestDb::with_modules(vec![
        crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
            .unwrap(),
    ])
    .await;
    let scope = map_scope(&db.a, "derivations").await;
    let foreign = map_scope(&db.a, "foreign").await;
    let writer = MapCatalog::new(db.a.clone());
    let reader = MapCatalog::new(db.b.clone());
    // Earlier foreign rows and same-tenant rows in another context must never
    // consume the caller's SQL limit or completion budget.
    for n in 0..110 {
        let value = raster(n);
        writer
            .put_raster_derivation(&foreign, &value)
            .await
            .unwrap();
        let mut value = value.into_value();
        value.work_context = WorkContextId::parse("private").unwrap();
        writer
            .put_raster_derivation(&scope, &RasterDerivation::new(value).unwrap())
            .await
            .unwrap();
    }
    let mut expected = Vec::new();
    for n in 256..381 {
        let value = raster(n);
        expected.push(value.derivation_id.to_string());
        writer.put_raster_derivation(&scope, &value).await.unwrap();
    }
    let original = raster(400);
    writer
        .put_raster_derivation(&scope, &original)
        .await
        .unwrap();
    writer
        .put_raster_derivation(&scope, &original)
        .await
        .unwrap();
    expected.push(original.derivation_id.to_string());
    let mut conflict = original.clone().into_value();
    conflict.output_checksum_sha256 = "e".repeat(64);
    let conflict = RasterDerivation::new(conflict).unwrap();
    assert!(
        writer
            .put_raster_derivation(&scope, &conflict)
            .await
            .is_err()
    );
    assert_eq!(
        reader
            .raster_derivation(&scope, &context(), &original.derivation_id)
            .await
            .unwrap(),
        Some(original.clone())
    );
    assert!(
        reader
            .raster_derivation(&foreign, &context(), &original.derivation_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        reader
            .raster_derivation(
                &scope,
                &WorkContextId::parse("private").unwrap(),
                &original.derivation_id
            )
            .await
            .unwrap()
            .is_none()
    );
    let other = spatial();
    writer.put_spatial_derivation(&scope, &other).await.unwrap();
    assert_eq!(
        reader
            .spatial_derivation(&scope, &context(), &other.derivation_id)
            .await
            .unwrap(),
        Some(other)
    );
    let first = reader
        .derivations_page(&scope, &context(), DerivationSelection::Raster(None))
        .await
        .unwrap();
    assert_eq!(first.items.len(), 100);
    let json = serde_json::to_string(&first).unwrap();
    assert!(
        !json.contains("sourceTransform"),
        "pages contain metadata, not full derivation documents"
    );
    let cursor = first.next_cursor.as_deref().unwrap();
    assert!(
        MapCatalogPage::SpatialDerivations { after: None }
            .resume(Some(cursor))
            .is_err()
    );
    assert!(
        MapCatalogPage::RasterDerivations { after: None }
            .resume(Some("not-hex"))
            .is_err()
    );
    let MapCatalogPage::RasterDerivations { after } =
        MapCatalogPage::RasterDerivations { after: None }
            .resume(Some(cursor))
            .unwrap()
    else {
        unreachable!()
    };

    let second = reader
        .derivations_page(
            &scope,
            &context(),
            DerivationSelection::Raster(after.as_ref()),
        )
        .await
        .unwrap();
    assert_eq!(second.items.len(), 26);
    assert!(second.next_cursor.is_none());
    let actual = first
        .items
        .into_iter()
        .chain(second.items)
        .map(|r| r.derivation_id)
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert_eq!(
        reader
            .complete_derivations(&scope, &context(), MapDerivationKind::Raster, "")
            .await
            .unwrap()
            .len(),
        101
    );
    // This match occurs beyond the first 100 authorized records.
    assert_eq!(
        reader
            .complete_derivations(&scope, &context(), MapDerivationKind::Raster, "0000017C")
            .await
            .unwrap(),
        vec![expected[124].clone()]
    );
    assert!(
        reader
            .complete_derivations(
                &scope,
                &context(),
                MapDerivationKind::Raster,
                "' OR true --"
            )
            .await
            .unwrap()
            .is_empty()
    );
    let mut plan =
        db.b.client()
            .query(include_str!(
                "../queries/derivations/tests/qualify_sql_pages/statement_1.surql"
            ))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind((
                "context",
                super::scope(&scope, &context())
                    .unwrap()
                    .work_context
                    .record_id(),
            ))
            .bind(("after", after.unwrap().as_str().to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap();
    let plan: Vec<serde_json::Value> = plan.take(0).unwrap();
    let plan = serde_json::to_string(&plan).unwrap();
    assert!(plan.contains("map_derivation_scope_key"), "{plan}");
    let repo = MapRepository::new(db.a.clone());
    let selected_scope = crate::persistence::MapDerivationScope::from_keys(
        &scope.identity.tenant_key,
        context().as_str(),
    )
    .unwrap();
    let selected_id =
        crate::persistence::MapDerivationIdentity::Raster(original.derivation_id.clone());
    let selected = repo
        .map_derivation(selected_scope, &selected_id)
        .await
        .unwrap()
        .unwrap();
    for field in ["createdBy", "createdAt", "derivationId"] {
        let mut body = serde_json::to_value(&original).unwrap();
        body[field] = match field {
            "createdBy" => serde_json::json!(PrincipalId::parse("other").unwrap()),
            "createdAt" => serde_json::json!(original.created_at + chrono::TimeDelta::seconds(1)),
            "derivationId" => serde_json::json!(RasterDerivationId::new()),
            _ => unreachable!(),
        };
        let corrupt = serde_json::to_string(&body).unwrap();
        db.a.client()
            .query(include_str!(
                "../queries/derivations/tests/corrupt_document.surql"
            ))
            .bind(("record", selected.id.clone()))
            .bind(("canonical_json", corrupt.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            reader
                .raster_derivation(&scope, &context(), &original.derivation_id)
                .await
                .is_err(),
            "{field}"
        );
        assert!(
            reader
                .raster_derivation(&foreign, &context(), &original.derivation_id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            repo.map_derivation(selected_scope, &selected_id)
                .await
                .unwrap()
                .unwrap()
                .canonical_json,
            corrupt,
            "reader must not repair retained bytes"
        );
    }
    db.a.client()
        .query(include_str!(
            "../queries/derivations/tests/corrupt_document.surql"
        ))
        .bind(("record", selected.id))
        .bind(("canonical_json", selected.canonical_json))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn store_invalidations_reach_another_replica_and_restart_without_discovery_churn() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let scope = map_scope(&db.a, "notifications").await;
        let writer = MapCatalog::new(db.a.clone());
        let hub = Arc::new(SubscriptionHub::new());
        let mut changes = hub.listen();
        let mut lists = hub.listen_resource_list_changes();
        for n in 0..2 {
            let stop = CancellationToken::new();
            let observer = tokio::spawn(crate::resource_changes::observe(
                db.b.clone(),
                hub.clone(),
                stop.clone(),
            ));
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(30), changes.recv())
                    .await
                    .unwrap()
                    .unwrap(),
                ResourceUpdate::Reconcile
            ));
            while changes.try_recv().is_ok() {}
            writer
                .put_raster_derivation(&scope, &raster(n))
                .await
                .unwrap();
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(30), changes.recv())
                    .await
                    .unwrap()
                    .unwrap(),
                ResourceUpdate::Reconcile
            ));
            stop.cancel();
            observer.await.unwrap();
            while changes.try_recv().is_ok() {}
            assert!(matches!(
                lists.try_recv(),
                Err(tokio::sync::broadcast::error::TryRecvError::Empty)
            ));
            writer
                .put_spatial_derivation(&scope, &spatial())
                .await
                .unwrap();
        }
    })
    .await
    .expect("Map observer qualification exceeded 120 seconds");
}
