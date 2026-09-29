use std::{sync::Arc, time::Duration};

use duckdb::params;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{ResourceUpdate, SubscriptionHub};
use veoveo_platform_store::PrincipalKind;

use super::*;
use crate::{
    analytics::{MapAnalytics, MapAnalyticsConfig},
    contract::*,
    test_store::TestDb,
};

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
    WorkContextId::new("operations").unwrap()
}
fn raster(n: usize) -> RasterDerivation {
    RasterDerivation {
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
        created_by: PrincipalId::new("author").unwrap(),
        work_context: context(),
        created_at: Utc::now(),
    }
}
fn spatial() -> SpatialDerivation {
    let id = SpatialDerivationId::new();
    SpatialDerivation {
        schema_version: SPATIAL_DERIVATION_SCHEMA_VERSION,
        resource_uri: crate::contract::MapSpatialDerivationUri::new(id.clone()).to_string(),
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
        created_by: PrincipalId::new("author").unwrap(),
        work_context: context(),
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn sql_pages_completion_and_immutable_reads_cross_replicas() {
    tokio::time::timeout(Duration::from_secs(180), qualify_sql_pages())
        .await
        .expect("Map SQL qualification exceeded 180 seconds");
}

async fn qualify_sql_pages() {
    let db = TestDb::new().await;
    let scope = map_scope(&db.a, "derivations").await;
    let foreign = map_scope(&db.a, "foreign").await;
    let writer = MapCatalog::new(db.a.clone());
    let reader = MapCatalog::new(db.b.clone());
    // Earlier foreign rows and same-tenant rows in another context must never
    // consume the caller's SQL limit or completion budget.
    for n in 0..110 {
        let mut value = raster(n);
        writer
            .put_raster_derivation(&foreign, &value)
            .await
            .unwrap();
        value.work_context = WorkContextId::new("private").unwrap();
        writer.put_raster_derivation(&scope, &value).await.unwrap();
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
    let mut conflict = original.clone();
    conflict.output_checksum_sha256 = "e".repeat(64);
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
                &WorkContextId::new("private").unwrap(),
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
        .derivations_page(&scope, &context(), MapDerivationKind::Raster, None)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 100);
    let json = serde_json::to_string(&first).unwrap();
    assert!(
        !json.contains("source_transform"),
        "pages contain metadata, not full derivation documents"
    );
    let cursor = first.next_cursor.as_deref().unwrap();
    assert!(parse_cursor(MapDerivationKind::Spatial, Some(cursor)).is_err());
    assert!(parse_cursor(MapDerivationKind::Raster, Some("not-hex")).is_err());
    let after = parse_cursor(MapDerivationKind::Raster, Some(cursor)).unwrap();
    let second = reader
        .derivations_page(
            &scope,
            &context(),
            MapDerivationKind::Raster,
            after.as_deref(),
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
    let mut plan=db.b.client().query("SELECT derivation_key, created_by, created_at FROM map_derivation WHERE tenant = $tenant AND work_context = $context AND kind = 'raster' AND derivation_key > $after ORDER BY derivation_key ASC LIMIT 101 EXPLAIN;")
            .bind(("tenant",scope.identity.tenant_id.record_id())).bind(("context",super::scope(&scope,&context()).unwrap().work_context.record_id())).bind(("after",after.unwrap())).await.unwrap().check().unwrap();
    let plan: Vec<serde_json::Value> = plan.take(0).unwrap();
    let plan = serde_json::to_string(&plan).unwrap();
    assert!(plan.contains("map_derivation_scope_key"), "{plan}");
}

fn analytics(root: &TempDir) -> MapAnalytics {
    MapAnalytics::open(MapAnalyticsConfig {
        database_path: root.path().join("map.duckdb"),
        authoring_task_root: root.path().join("tasks"),
        spill_dir: root.path().join("spill"),
        spatial_extension: std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION")
            .expect("pinned Spatial extension required")
            .into(),
        memory_limit: "256MB".into(),
        threads: 1,
    })
    .unwrap()
}
fn legacy_insert(
    analytics: &MapAnalytics,
    table: &str,
    scope: &MapAccessContext,
    context: &str,
    key: &str,
    json: &str,
) {
    analytics
        .connection()
        .unwrap()
        .execute(
            &format!("INSERT INTO {table} VALUES (?, ?, ?, ?, ?)"),
            params![scope.tenant_key(), context, "author", key, json],
        )
        .unwrap();
}
fn version(analytics: &MapAnalytics) -> i64 {
    analytics
        .read_connection()
        .unwrap()
        .query_row("SELECT version FROM map_schema", [], |row| row.get(0))
        .unwrap()
}

#[tokio::test]
async fn legacy_transfer_preserves_rows_until_all_records_are_durable_and_resumes() {
    if std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION").is_none() {
        return;
    }
    tokio::time::timeout(Duration::from_secs(180), qualify_legacy_transfer())
        .await
        .expect("Map transfer qualification exceeded 180 seconds");
}

async fn qualify_legacy_transfer() {
    let db = TestDb::new().await;
    let scope = map_scope(&db.a, "transfer").await;
    let writer = MapCatalog::new(db.a.clone());
    let reader = MapCatalog::new(db.b.clone());
    let root = TempDir::new().unwrap();
    let local = analytics(&root);
    local.connection().unwrap().execute_batch("CREATE TABLE map_raster_derivation (tenant_key VARCHAR, work_context_key VARCHAR, principal_key VARCHAR, derivation_key VARCHAR, canonical_json VARCHAR); CREATE TABLE map_spatial_derivation (tenant_key VARCHAR, work_context_key VARCHAR, principal_key VARCHAR, derivation_key VARCHAR, canonical_json VARCHAR); UPDATE map_schema SET version = 10;").unwrap();
    let mut records = Vec::new();
    // Cross the migration's 16-record page boundary before inducing failure.
    for n in 0..18 {
        let value = raster(n);
        legacy_insert(
            &local,
            "map_raster_derivation",
            &scope,
            "operations",
            value.derivation_id.as_str(),
            &serde_json::to_string(&value).unwrap(),
        );
        records.push(value);
    }
    let spatial = spatial();
    let key = spatial.derivation_id.as_str();
    legacy_insert(
        &local,
        "map_spatial_derivation",
        &scope,
        "operations",
        key,
        "invalid-json",
    );
    assert!(writer.migrate_local_derivations(&local).await.is_err());
    assert_eq!(version(&local), 10);
    assert_eq!(
        local
            .read_connection()
            .unwrap()
            .query_row("SELECT count(*) FROM map_raster_derivation", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        18
    );
    assert_eq!(
        reader
            .raster_derivation(&scope, &context(), &records[17].derivation_id)
            .await
            .unwrap(),
        Some(records[17].clone())
    );
    local
        .connection()
        .unwrap()
        .execute(
            "UPDATE map_spatial_derivation SET canonical_json = ?",
            params![serde_json::to_string(&spatial).unwrap()],
        )
        .unwrap();
    // Schema 9 rebuilds its R-trees before resuming the same transfer.
    local
        .connection()
        .unwrap()
        .execute_batch("UPDATE map_schema SET version = 9;")
        .unwrap();
    drop(local);
    let local = analytics(&root);
    assert_eq!(version(&local), 10);
    writer.migrate_local_derivations(&local).await.unwrap();
    assert_eq!(version(&local), 11);
    for record in records {
        assert_eq!(
            reader
                .raster_derivation(&scope, &context(), &record.derivation_id)
                .await
                .unwrap(),
            Some(record)
        );
    }
    assert_eq!(
        reader
            .spatial_derivation(&scope, &context(), &spatial.derivation_id)
            .await
            .unwrap(),
        Some(spatial)
    );
    assert!(
        local
            .read_connection()
            .unwrap()
            .prepare("SELECT * FROM map_raster_derivation")
            .is_err()
    );
    assert!(
        local
            .read_connection()
            .unwrap()
            .prepare("SELECT * FROM map_spatial_derivation")
            .is_err()
    );
    writer.migrate_local_derivations(&local).await.unwrap();
}

#[tokio::test]
async fn store_invalidations_reach_another_replica_and_restart_without_discovery_churn() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = TestDb::new().await;
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
