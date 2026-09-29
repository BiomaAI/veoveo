//! Exercise committed Spatial WAL replay through the production Map opener.
use std::{path::Path, process::Stdio, time::Duration};

use tempfile::TempDir;

use super::{MapAnalytics, MapAnalyticsConfig, SCHEMA_VERSION};

const CHILD_ROOT: &str = "VEOVEO_TEST_MAP_CRASH_ROOT";
const CHILD_EXIT: i32 = 37;

fn open(root: &Path, extension: &std::ffi::OsStr) -> MapAnalytics {
    MapAnalytics::open(MapAnalyticsConfig {
        database_path: root.join("map.duckdb"),
        authoring_task_root: root.join("tasks"),
        spill_dir: root.join("spill"),
        spatial_extension: extension.into(),
        memory_limit: "256MB".into(),
        threads: 1,
    })
    .unwrap()
}

#[tokio::test]
async fn committed_spatial_wal_survives_process_exit() {
    let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
        return;
    };
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let analytics = open(Path::new(&root), &extension);
        let connection = analytics.connection().unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO map_source_feature (
                  tenant_key, release_key, feature_key, source_key,
                  source_element_type, source_element_key, source_element_version,
                  representation, geometry_digest_sha256, geometry, normalized_text,
                  tags_json, canonical_json, source_digest_sha256,
                  projection_attempt_key, projection_ordinal
                ) VALUES
                  ('tenant', 'release', 'point', 'source', 'node', 'point', '1',
                   'center', 'point-digest', ST_GeomFromGeoJSON('{"type":"Point","coordinates":[-89.214,13.696]}'),
                   'point', '{}', '{}', 'source-digest', 'attempt', 0),
                  ('tenant', 'release', 'line', 'source', 'way', 'line', '1',
                   'centerline', 'line-digest', ST_GeomFromGeoJSON('{"type":"LineString","coordinates":[[-89.22,13.69],[-89.21,13.70]]}'),
                   'line', '{}', '{}', 'source-digest', 'attempt', 1),
                  ('tenant', 'release', 'polygon', 'source', 'relation', 'polygon', '1',
                   'footprint', 'polygon-digest', ST_GeomFromGeoJSON('{"type":"Polygon","coordinates":[[[-89.22,13.69],[-89.21,13.69],[-89.21,13.70],[-89.22,13.70],[-89.22,13.69]]]}'),
                   'polygon', '{}', '{}', 'source-digest', 'attempt', 2);
                BEGIN;
                DELETE FROM map_source_feature;
                "#,
            )
            .unwrap();
        // Skip all Rust destructors and DuckDB's close/checkpoint. The committed
        // mixed-geometry insert must replay; the open deletion must roll back.
        std::process::exit(CHILD_EXIT);
    }

    let root = TempDir::new().unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "analytics::recovery_tests::committed_spatial_wal_survives_process_exit",
                "--nocapture",
            ])
            .env(CHILD_ROOT, root.path())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("Map crash fixture exceeded 30 seconds")
    .unwrap();
    assert_eq!(
        output.status.code(),
        Some(CHILD_EXIT),
        "child stdout: {}\nchild stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        root.path().join("map.duckdb.wal").metadata().unwrap().len() > 0,
        "the fixture must recover committed WAL, not a clean checkpoint"
    );

    // First open replays the WAL. The second qualifies the recovered checkpoint.
    for _ in 0..2 {
        let analytics = open(root.path(), &extension);
        let connection = analytics.connection().unwrap();
        let version: i64 = connection
            .query_row("SELECT version FROM map_schema", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let stored: u64 = connection
            .query_row("SELECT count(*) FROM map_source_feature", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stored, 3);
        let indexed: u64 = connection
            .query_row(
                "SELECT count(*) FROM rtree_index_dump('map_source_feature_geometry') WHERE row_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, stored);
        let mut statement = connection
            .prepare("SELECT feature_key FROM map_source_feature WHERE ST_Intersects(geometry, ST_MakeEnvelope(-89.23, 13.68, -89.20, 13.71)) ORDER BY feature_key")
            .unwrap();
        let keys: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(keys, ["line", "point", "polygon"]);
    }
}
