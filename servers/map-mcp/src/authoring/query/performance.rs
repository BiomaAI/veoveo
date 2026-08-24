use std::time::{Duration, Instant};

use duckdb::{Connection, params, params_from_iter};
use tempfile::TempDir;

use crate::{
    analytics::{MapAnalytics, MapAnalyticsConfig},
    contract::{FeatureLayerId, QueryFeaturesRequest, Wgs84BoundingBox},
};

use super::{PreparedFeatureQuery, build_feature_query};

const TENANT: &str = "performance-tenant";
const WORK_CONTEXT: &str = "performance-work-context";
const MAX_COLD_QUERY: Duration = Duration::from_secs(2);
const MAX_WARM_P95_QUERY: Duration = Duration::from_millis(250);
const MIN_INDEXED_INSERT_ROWS_PER_SECOND: f64 = 5_000.0;
const MAX_DATABASE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[test]
fn million_feature_rtree_plan_correctness_and_latency_budget() {
    let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
        eprintln!("skipping million-feature R-tree gate without pinned Spatial extension");
        return;
    };
    let root = TempDir::new().expect("temporary performance database");
    let analytics = MapAnalytics::open(MapAnalyticsConfig {
        database_path: root.path().join("map.duckdb"),
        authoring_task_root: root.path().join("tasks"),
        spill_dir: root.path().join("spill"),
        spatial_extension: extension.into(),
        memory_limit: "1GB".to_owned(),
        threads: 4,
    })
    .expect("configured DuckDB Spatial projection");
    let connection = analytics.connection().expect("projection connection");
    let layer_id = FeatureLayerId::new();
    let request = selective_request(layer_id.clone());

    let started = Instant::now();
    let mut previous_scale = 0_u64;
    for scale in [10_000_u64, 100_000, 1_000_000] {
        insert_head_batch(
            &connection,
            &layer_id,
            previous_scale,
            scale - previous_scale,
        );
        previous_scale = scale;
        connection
            .execute_batch("ANALYZE map_authored_feature_head")
            .expect("analyze authored head projection");
        let prepared = build_feature_query(TENANT, WORK_CONTEXT, &request, None)
            .expect("current feature query");
        let plan = explain(&connection, &prepared);
        assert!(
            plan.contains("RTREE_INDEX_SCAN") && plan.contains("map_authored_head_geometry"),
            "selective current-feature query lost its R-tree scan at {scale} rows:\n{plan}"
        );
        assert_spatial_count_matches_bbox_oracle(&connection, scale, &request);
        let cold = timed_query(&connection, &prepared);
        assert!(
            cold <= MAX_COLD_QUERY,
            "cold selective query at {scale} rows took {cold:?}, budget {MAX_COLD_QUERY:?}"
        );
        let p95 = warm_p95(&connection, &prepared, 20);
        assert!(
            p95 <= MAX_WARM_P95_QUERY,
            "warm selective query p95 at {scale} rows took {p95:?}, budget {MAX_WARM_P95_QUERY:?}"
        );
        eprintln!("authored R-tree scale={scale} cold={cold:?} warm_p95={p95:?}");
    }
    let insert_rate = 1_000_000.0 / started.elapsed().as_secs_f64();
    assert!(
        insert_rate >= MIN_INDEXED_INSERT_ROWS_PER_SECOND,
        "indexed projection insert rate {insert_rate:.0} rows/s is below {MIN_INDEXED_INSERT_ROWS_PER_SECOND:.0} rows/s"
    );

    assert_head_delete_and_reinsert_correctness(&connection, &request);
    assert_dateline_plan_and_correctness(&connection, &layer_id);
    assert_publication_plan_and_latest_revision_correctness(&connection, &layer_id);
    assert_index_structure_and_storage_bound(&analytics, &connection);
    eprintln!("indexed projection insert_rate={insert_rate:.0} rows/s");
}

fn assert_head_delete_and_reinsert_correctness(
    connection: &Connection,
    request: &QueryFeaturesRequest,
) {
    let prepared =
        build_feature_query(TENANT, WORK_CONTEXT, request, None).expect("head maintenance query");
    let before = query_keys(connection, &prepared);
    let victim = before.first().expect("head fixture inside bbox").clone();
    connection
        .execute(
            "CREATE TEMP TABLE rtree_victim AS
             SELECT * FROM map_authored_feature_head
             WHERE tenant_key = ? AND layer_key = ? AND feature_key = ?",
            params![TENANT, request.layer_id.as_str(), victim],
        )
        .expect("retain head row for R-tree maintenance test");
    connection
        .execute(
            "DELETE FROM map_authored_feature_head
             WHERE tenant_key = ? AND layer_key = ? AND feature_key = ?",
            params![TENANT, request.layer_id.as_str(), victim],
        )
        .expect("delete indexed head row");
    let deleted = query_keys(connection, &prepared);
    assert_eq!(deleted.len() + 1, before.len());
    assert!(!deleted.contains(&victim));
    connection
        .execute_batch(
            "INSERT INTO map_authored_feature_head SELECT * FROM rtree_victim;
             DROP TABLE rtree_victim;",
        )
        .expect("reinsert indexed head row");
    let restored = query_keys(connection, &prepared);
    assert_eq!(restored.len(), before.len());
    assert!(restored.contains(&victim));
}

fn selective_request(layer_id: FeatureLayerId) -> QueryFeaturesRequest {
    QueryFeaturesRequest {
        layer_id,
        publication_id: None,
        bbox: Some(Wgs84BoundingBox {
            west: -2.0,
            south: -2.0,
            east: 2.0,
            north: 2.0,
        }),
        datetime: None,
        geometry_type: None,
        filter: None,
        limit: 1_000,
        cursor: None,
        minimum_commit_sequence: None,
    }
}

fn insert_head_batch(connection: &Connection, layer_id: &FeatureLayerId, start: u64, count: u64) {
    let width = (count as f64).sqrt().ceil() as u64;
    let height = count.div_ceil(width);
    let end = start + count;
    let sql = format!(
        "INSERT INTO map_authored_feature_head (
           tenant_key, work_context_key, layer_key, feature_key, feature_revision,
           layer_revision, schema_version, changeset_key, commit_sequence, deleted,
           geometry_type, geometry, bbox_west, bbox_south, bbox_east, bbox_north,
           valid_from, valid_until, semantic_type, title, properties_json,
           canonical_json, updated_at
         )
         SELECT ?, ?, ?, printf('fixture-%010d', i), 1, 1, 1,
           printf('changeset-%010d', i), i, false, 'point', ST_Point(lon, lat),
           lon, lat, lon, lat, NULL, NULL, 'Fixture', NULL, '{{}}'::JSON,
           '{{}}'::JSON, current_timestamp
         FROM (
           SELECT i,
             -180.0 + ((i - {start}) % {width} + 0.5) * (360.0 / {width}) AS lon,
             -90.0 + (floor((i - {start}) / {width}) + 0.5) * (180.0 / {height}) AS lat
           FROM range({start}, {end}) AS fixture(i)
         )"
    );
    connection
        .execute(&sql, params![TENANT, WORK_CONTEXT, layer_id.as_str()])
        .expect("insert indexed authored fixture batch");
    let stored: u64 = connection
        .query_row(
            "SELECT count(*) FROM map_authored_feature_head WHERE tenant_key = ? AND layer_key = ?",
            params![TENANT, layer_id.as_str()],
            |row| row.get(0),
        )
        .expect("count authored fixture rows");
    assert_eq!(stored, end);
}

fn explain(connection: &Connection, prepared: &PreparedFeatureQuery) -> String {
    let mut statement = connection
        .prepare(&format!("EXPLAIN {}", prepared.sql))
        .expect("prepare explain");
    let mut rows = statement
        .query(params_from_iter(prepared.parameters.iter()))
        .expect("explain authored query");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read explain row") {
        let value: String = row.get(1).expect("physical explain value");
        plan.push_str(&value);
        plan.push('\n');
    }
    plan
}

fn query_keys(connection: &Connection, prepared: &PreparedFeatureQuery) -> Vec<String> {
    let mut statement = connection
        .prepare(&prepared.sql)
        .expect("prepare authored feature query");
    let mut rows = statement
        .query(params_from_iter(prepared.parameters.iter()))
        .expect("execute authored feature query");
    let mut keys = Vec::new();
    while let Some(row) = rows.next().expect("read authored feature row") {
        keys.push(row.get(1).expect("projected feature key"));
    }
    keys
}

fn timed_query(connection: &Connection, prepared: &PreparedFeatureQuery) -> Duration {
    let started = Instant::now();
    let keys = query_keys(connection, prepared);
    assert!(!keys.is_empty(), "selective fixture query must return rows");
    started.elapsed()
}

fn warm_p95(connection: &Connection, prepared: &PreparedFeatureQuery, samples: usize) -> Duration {
    let mut durations = (0..samples)
        .map(|_| timed_query(connection, prepared))
        .collect::<Vec<_>>();
    durations.sort_unstable();
    durations[(samples * 95).div_ceil(100).saturating_sub(1)]
}

fn assert_spatial_count_matches_bbox_oracle(
    connection: &Connection,
    scale: u64,
    request: &QueryFeaturesRequest,
) {
    let bbox = request.bbox.as_ref().expect("test bbox");
    let spatial: u64 = connection
        .query_row(
            "SELECT count(*) FROM map_authored_feature_head
             WHERE tenant_key = ? AND work_context_key = ? AND layer_key = ?
               AND ST_Intersects(geometry, ST_MakeEnvelope(-2, -2, 2, 2))",
            params![TENANT, WORK_CONTEXT, request.layer_id.as_str()],
            |row| row.get(0),
        )
        .expect("spatial count");
    let oracle: u64 = connection
        .query_row(
            "SELECT count(*) FROM map_authored_feature_head
             WHERE tenant_key = ? AND work_context_key = ? AND layer_key = ?
               AND bbox_west BETWEEN ? AND ? AND bbox_south BETWEEN ? AND ?",
            params![
                TENANT,
                WORK_CONTEXT,
                request.layer_id.as_str(),
                bbox.west,
                bbox.east,
                bbox.south,
                bbox.north,
            ],
            |row| row.get(0),
        )
        .expect("numeric point oracle count");
    assert_eq!(
        spatial, oracle,
        "R-tree result differs from point bbox oracle"
    );
    assert!(
        spatial > 0 && spatial < scale / 100,
        "fixture query is not selective"
    );
}

fn assert_dateline_plan_and_correctness(connection: &Connection, layer_id: &FeatureLayerId) {
    let mut request = selective_request(layer_id.clone());
    request.bbox = Some(Wgs84BoundingBox {
        west: 179.0,
        south: -1.0,
        east: -179.0,
        north: 1.0,
    });
    let prepared =
        build_feature_query(TENANT, WORK_CONTEXT, &request, None).expect("dateline feature query");
    let plan = explain(connection, &prepared);
    assert!(
        plan.matches("RTREE_INDEX_SCAN").count() >= 2,
        "dateline query must retain an R-tree scan for both longitude segments:\n{plan}"
    );
    let spatial: u64 = connection
        .query_row(
            "SELECT count(*) FROM map_authored_feature_head
             WHERE tenant_key = ? AND layer_key = ? AND
               (ST_Intersects(geometry, ST_MakeEnvelope(179, -1, 180, 1)) OR
                ST_Intersects(geometry, ST_MakeEnvelope(-180, -1, -179, 1)))",
            params![TENANT, layer_id.as_str()],
            |row| row.get(0),
        )
        .expect("dateline spatial count");
    let oracle: u64 = connection
        .query_row(
            "SELECT count(*) FROM map_authored_feature_head
             WHERE tenant_key = ? AND layer_key = ? AND bbox_south BETWEEN -1 AND 1
               AND (bbox_west >= 179 OR bbox_west <= -179)",
            params![TENANT, layer_id.as_str()],
            |row| row.get(0),
        )
        .expect("dateline numeric oracle");
    assert_eq!(spatial, oracle);
}

fn assert_publication_plan_and_latest_revision_correctness(
    connection: &Connection,
    layer_id: &FeatureLayerId,
) {
    connection
        .execute(
            "INSERT INTO map_authored_feature_revision
             SELECT * FROM map_authored_feature_head
             WHERE tenant_key = ? AND layer_key = ? AND feature_key < 'fixture-0000100000'",
            params![TENANT, layer_id.as_str()],
        )
        .expect("seed 100k publication revisions");
    connection
        .execute_batch("ANALYZE map_authored_feature_revision")
        .expect("analyze publication revisions");
    let request = selective_request(layer_id.clone());
    let revision_one =
        build_feature_query(TENANT, WORK_CONTEXT, &request, Some(1)).expect("publication query");
    let plan = explain(connection, &revision_one);
    assert!(
        plan.contains("RTREE_INDEX_SCAN"),
        "publication query must use the revision R-tree candidate scan:\n{plan}"
    );
    let initial = query_keys(connection, &revision_one);
    let victim = initial
        .first()
        .expect("publication fixture inside bbox")
        .clone();
    connection
        .execute(
            "INSERT INTO map_authored_feature_revision
             SELECT * REPLACE (
               2 AS feature_revision, 2 AS layer_revision, 2 AS commit_sequence,
               ST_Point(50, 50) AS geometry,
               50.0 AS bbox_west, 50.0 AS bbox_south,
               50.0 AS bbox_east, 50.0 AS bbox_north,
               current_timestamp AS created_at
             )
             FROM map_authored_feature_revision
             WHERE tenant_key = ? AND layer_key = ? AND feature_key = ? AND feature_revision = 1",
            params![TENANT, layer_id.as_str(), victim],
        )
        .expect("append moved publication revision");
    let revision_two = build_feature_query(TENANT, WORK_CONTEXT, &request, Some(2))
        .expect("updated publication query");
    let latest = query_keys(connection, &revision_two);
    assert!(
        !latest.contains(&victim),
        "spatial candidate scan must not return an older in-bounds revision after the latest moved"
    );
    let oracle: u64 = connection
        .query_row(
            "SELECT count(*) FROM (
               SELECT *, row_number() OVER (
                 PARTITION BY feature_key ORDER BY feature_revision DESC
               ) AS version_rank
               FROM map_authored_feature_revision
               WHERE tenant_key = ? AND work_context_key = ? AND layer_key = ? AND layer_revision <= 2
             )
             WHERE version_rank = 1 AND bbox_west BETWEEN -2 AND 2
               AND bbox_south BETWEEN -2 AND 2",
            params![TENANT, WORK_CONTEXT, layer_id.as_str()],
            |row| row.get(0),
        )
        .expect("latest-revision numeric oracle");
    assert_eq!(latest.len() as u64, oracle);
}

fn assert_index_structure_and_storage_bound(analytics: &MapAnalytics, connection: &Connection) {
    let leaf_entries: u64 = connection
        .query_row(
            "SELECT count(*) FROM rtree_index_dump('map_authored_head_geometry') WHERE row_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("inspect R-tree leaves");
    assert_eq!(leaf_entries, 1_000_000);
    connection
        .execute_batch("CHECKPOINT")
        .expect("checkpoint fixture");
    let bytes = std::fs::metadata(analytics.database_path())
        .expect("performance database metadata")
        .len();
    assert!(
        bytes <= MAX_DATABASE_BYTES,
        "million-feature projection occupies {bytes} bytes, budget {MAX_DATABASE_BYTES}"
    );
    eprintln!("million-feature projection database_bytes={bytes}");
}
