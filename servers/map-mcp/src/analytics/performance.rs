use std::time::{Duration, Instant};

use duckdb::{Connection, params};
use tempfile::TempDir;

use crate::{
    analytics::{MapAnalytics, MapAnalyticsConfig, source_feature_query_sql},
    contract::{
        DatasetReleaseId, MapSourceId, QuerySourceFeaturesRequest, SourceSpatialQuery,
        Wgs84BoundingBox,
    },
};

const TENANT: &str = "source-performance-tenant";
const PROJECTION_ATTEMPT: &str = "source-performance-attempt";
const MAX_COLD_QUERY: Duration = Duration::from_secs(2);
const MAX_WARM_P95_QUERY: Duration = Duration::from_millis(250);
const MIN_INDEXED_INSERT_ROWS_PER_SECOND: f64 = 5_000.0;
const MAX_DATABASE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[test]
fn million_source_feature_rtree_plan_correctness_and_latency_budget() {
    let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
        eprintln!("skipping million-source-feature R-tree gate without pinned Spatial extension");
        return;
    };
    let root = TempDir::new().expect("temporary source performance database");
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
    let release_id = DatasetReleaseId::new();
    let source_id = MapSourceId::new();
    connection
        .execute(
            "INSERT INTO map_release_projection VALUES (?, ?, ?, 0, 0, 0, 0, 0, 0, current_timestamp)",
            params![TENANT, release_id.as_str(), PROJECTION_ATTEMPT],
        )
        .expect("visible source projection marker");
    let request = selective_request(release_id.clone(), source_id.clone());

    let started = Instant::now();
    let mut previous_scale = 0_u64;
    for scale in [10_000_u64, 100_000, 1_000_000] {
        insert_source_batch(
            &connection,
            &release_id,
            &source_id,
            previous_scale,
            scale - previous_scale,
        );
        previous_scale = scale;
        connection
            .execute_batch("ANALYZE map_source_feature")
            .expect("analyze source feature projection");
        let sql = source_feature_query_sql(TENANT, &request, None).expect("source feature query");
        let plan = explain(&connection, &sql);
        assert!(
            plan.contains("RTREE_INDEX_SCAN") && plan.contains("map_source_feature_geometry"),
            "selective source query lost its R-tree scan at {scale} rows:\n{plan}"
        );
        assert_spatial_count_matches_scan_oracle(&connection, &release_id, scale);
        let cold = timed_query(&connection, &sql);
        assert!(
            cold <= MAX_COLD_QUERY,
            "cold selective source query at {scale} rows took {cold:?}, budget {MAX_COLD_QUERY:?}"
        );
        let p95 = warm_p95(&connection, &sql, 20);
        assert!(
            p95 <= MAX_WARM_P95_QUERY,
            "warm selective source query p95 at {scale} rows took {p95:?}, budget {MAX_WARM_P95_QUERY:?}"
        );
        eprintln!("source R-tree scale={scale} cold={cold:?} warm_p95={p95:?}");
    }
    let insert_rate = 1_000_000.0 / started.elapsed().as_secs_f64();
    assert!(
        insert_rate >= MIN_INDEXED_INSERT_ROWS_PER_SECOND,
        "indexed source insert rate {insert_rate:.0} rows/s is below {MIN_INDEXED_INSERT_ROWS_PER_SECOND:.0} rows/s"
    );
    assert_dateline_plan(&connection, &release_id, &source_id);
    assert_index_structure_and_storage_bound(&analytics, &connection);
    eprintln!("indexed source projection insert_rate={insert_rate:.0} rows/s");
}

fn selective_request(
    release_id: DatasetReleaseId,
    source_id: MapSourceId,
) -> QuerySourceFeaturesRequest {
    QuerySourceFeaturesRequest {
        release_id,
        source_id: Some(source_id),
        source_element_id: None,
        representation: None,
        tags_equal: Vec::new(),
        tags_exist: Vec::new(),
        normalized_text: None,
        spatial: Some(SourceSpatialQuery::BoundingBox {
            bounds: Wgs84BoundingBox {
                west: -2.0,
                south: -2.0,
                east: 2.0,
                north: 2.0,
            },
        }),
        limit: 500,
        cursor: None,
    }
}

fn insert_source_batch(
    connection: &Connection,
    release_id: &DatasetReleaseId,
    source_id: &MapSourceId,
    start: u64,
    count: u64,
) {
    let width = (count as f64).sqrt().ceil() as u64;
    let height = count.div_ceil(width);
    let end = start + count;
    let sql = format!(
        "INSERT INTO map_source_feature (
           tenant_key, release_key, feature_key, source_key, source_element_type,
           source_element_key, source_element_version, representation,
           geometry_digest_sha256, geometry, normalized_text, tags_json,
           canonical_json, source_digest_sha256, projection_attempt_key,
           projection_ordinal
         )
         SELECT ?, ?, printf('source-feature-%010d', i), ?, 'feature',
           printf('source-element-%010d', i), '1', 'point', repeat('a', 64),
           ST_Point(lon, lat), '', '{{}}'::JSON, json_object('fixture', i),
           repeat('b', 64), ?, i
         FROM (
           SELECT i,
             -180.0 + ((i - {start}) % {width} + 0.5) * (360.0 / {width}) AS lon,
             -90.0 + (floor((i - {start}) / {width}) + 0.5) * (180.0 / {height}) AS lat
           FROM range({start}, {end}) AS fixture(i)
         )"
    );
    connection
        .execute(
            &sql,
            params![
                TENANT,
                release_id.as_str(),
                source_id.as_str(),
                PROJECTION_ATTEMPT
            ],
        )
        .expect("insert indexed source fixture batch");
    let stored: u64 = connection
        .query_row(
            "SELECT count(*) FROM map_source_feature WHERE tenant_key = ? AND release_key = ?",
            params![TENANT, release_id.as_str()],
            |row| row.get(0),
        )
        .expect("count source fixture rows");
    assert_eq!(stored, end);
}

fn explain(connection: &Connection, sql: &str) -> String {
    let mut statement = connection
        .prepare(&format!("EXPLAIN {sql}"))
        .expect("prepare source explain");
    let mut rows = statement.query([]).expect("explain source query");
    let mut plan = String::new();
    while let Some(row) = rows.next().expect("read source explain row") {
        let value: String = row.get(1).expect("physical explain value");
        plan.push_str(&value);
        plan.push('\n');
    }
    plan
}

fn timed_query(connection: &Connection, sql: &str) -> Duration {
    let started = Instant::now();
    let mut statement = connection.prepare(sql).expect("prepare source query");
    let mut rows = statement.query([]).expect("execute source query");
    let mut count = 0_u64;
    while let Some(row) = rows.next().expect("read source query row") {
        let _: String = row.get(0).expect("source canonical JSON");
        count += 1;
    }
    assert!(count > 0 && count <= 501, "bounded source preview query");
    started.elapsed()
}

fn warm_p95(connection: &Connection, sql: &str, samples: usize) -> Duration {
    let mut durations = (0..samples)
        .map(|_| timed_query(connection, sql))
        .collect::<Vec<_>>();
    durations.sort_unstable();
    durations[(samples * 95).div_ceil(100).saturating_sub(1)]
}

fn assert_spatial_count_matches_scan_oracle(
    connection: &Connection,
    release_id: &DatasetReleaseId,
    scale: u64,
) {
    let spatial: u64 = connection
        .query_row(
            "SELECT count(*) FROM map_source_feature
             WHERE tenant_key = ? AND release_key = ?
               AND ST_Intersects(geometry, ST_MakeEnvelope(-2, -2, 2, 2))",
            params![TENANT, release_id.as_str()],
            |row| row.get(0),
        )
        .expect("indexed source spatial count");
    let oracle: u64 = connection
        .query_row(
            "SELECT count(*) FROM map_source_feature
             WHERE tenant_key = ? AND release_key = ?
               AND ST_X(ST_Centroid(geometry)) BETWEEN -2 AND 2
               AND ST_Y(ST_Centroid(geometry)) BETWEEN -2 AND 2",
            params![TENANT, release_id.as_str()],
            |row| row.get(0),
        )
        .expect("source full-scan point oracle");
    assert_eq!(spatial, oracle, "source R-tree differs from scan oracle");
    assert!(
        spatial > 0 && spatial < scale / 100,
        "source fixture query is not selective"
    );
}

fn assert_dateline_plan(
    connection: &Connection,
    release_id: &DatasetReleaseId,
    source_id: &MapSourceId,
) {
    let mut request = selective_request(release_id.clone(), source_id.clone());
    request.spatial = Some(SourceSpatialQuery::BoundingBox {
        bounds: Wgs84BoundingBox {
            west: 179.0,
            south: -1.0,
            east: -179.0,
            north: 1.0,
        },
    });
    let sql = source_feature_query_sql(TENANT, &request, None).expect("dateline source query");
    let plan = explain(connection, &sql);
    assert!(
        plan.matches("RTREE_INDEX_SCAN").count() >= 2,
        "dateline source preview must retain an R-tree scan for both longitude segments:\n{plan}"
    );
}

fn assert_index_structure_and_storage_bound(analytics: &MapAnalytics, connection: &Connection) {
    let leaf_entries: u64 = connection
        .query_row(
            "SELECT count(*) FROM rtree_index_dump('map_source_feature_geometry') WHERE row_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("inspect source R-tree leaves");
    assert_eq!(leaf_entries, 1_000_000);
    connection
        .execute_batch("CHECKPOINT")
        .expect("checkpoint source fixture");
    let bytes = std::fs::metadata(analytics.database_path())
        .expect("source performance database metadata")
        .len();
    assert!(
        bytes <= MAX_DATABASE_BYTES,
        "million-source-feature projection occupies {bytes} bytes, budget {MAX_DATABASE_BYTES}"
    );
    eprintln!("million-source-feature projection database_bytes={bytes}");
}
