#[path = "contract/catalog.rs"]
mod catalog;
#[path = "contract/resources.rs"]
mod resources;
use serde_json::{Value, json};
use veoveo_duckdb_mcp::contract::*;

#[test]
fn schemas_match_the_declared_contract() {
    let baseline: Value =
        serde_json::from_str(include_str!("../testdata/source-contract.schema.json")).unwrap();
    macro_rules! check {
        ($($ty:ty),+ $(,)?) => { $(
            assert_eq!(serde_json::to_value(schemars::schema_for!($ty)).unwrap(), baseline[stringify!($ty)], stringify!($ty));
        )+ };
    }
    check!(
        DuckDbFormat,
        DuckDbReadOptions,
        DuckDbSource,
        DuckDbDatabaseId,
        DuckDbExportFormat,
        DuckDbQueryOutputMode,
        DuckDbQueryRequest,
        DuckDbExecuteRequest,
        DuckDbIngestMode,
        DuckDbIngestRequest,
        DuckDbExportSelection,
        DuckDbExportRequest,
        DuckDbColumn,
        DuckDbQueryOutput,
        DuckDbExecuteOutput,
        DuckDbIngestOutput,
        DuckDbExportOutput,
        DuckDbDatabaseUri,
        DuckDbResource,
        DuckDbDatabaseCursor,
        DuckDbDatabaseEntry,
        DuckDbDatabasePage,
        DuckDbDatabaseSchema,
        DuckDbTableSchema,
        DuckDbSchemaColumn
    );
}

#[test]
fn public_consumer_reuses_source_variants_and_default_ingest_fields() {
    for source in [
        json!({"kind":"inline_csv", "csv":"value\n1\n", "filename":"measurements.csv", "options":{"header":true}}),
        json!({"kind":"uri", "uri":"https://data.example.test/measurements.parquet", "format":"parquet", "options":{}}),
        json!({"kind":"uris", "uris":["https://data.example.test/part1.csv", "https://data.example.test/part2.csv"], "format":"csv", "options":{"delimiter":","}}),
        json!({"kind":"artifact", "uri":"artifact://01900000-0000-7000-8000-000000000001", "format":"ndjson", "options":{}}),
    ] {
        let decoded: DuckDbSource = serde_json::from_value(source.clone()).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), source);
        let request: DuckDbIngestRequest = serde_json::from_value(json!({
            "db":"measurements", "table":"observations", "source":source, "mode":"create",
        }))
        .unwrap();
        assert_eq!(request.source, decoded);
        assert_eq!(request.db, DuckDbDatabaseId::new("measurements").unwrap());
        assert!(!request.create_db_if_missing);
        assert_eq!(
            serde_json::from_value::<DuckDbIngestRequest>(serde_json::to_value(&request).unwrap())
                .unwrap(),
            request
        );
    }
    let query: DuckDbQueryRequest =
        serde_json::from_value(json!({"db":"measurements", "sql":"SELECT 1"})).unwrap();
    assert_eq!(query.output, DuckDbQueryOutputMode::Inline);
    assert!(query.attach.is_empty());
    assert!(query.row_limit.is_none());
    assert!(query.timeout_ms.is_none());
    for id in ["", "../private", "UPPER", "0db", "has-dash"] {
        assert!(
            serde_json::from_value::<DuckDbQueryRequest>(json!({"db":id,"sql":"SELECT 1"}))
                .is_err()
        );
    }
}

#[test]
fn read_fragments_keep_quoting_and_option_admission() {
    assert_eq!(duckdb_quote_identifier("odd\"name"), "\"odd\"\"name\"");
    let source = duckdb_quote_literal("a'); DROP TABLE observations; --.parquet");
    assert_eq!(
        duckdb_read_function_sql(
            &source,
            &DuckDbFormat::Parquet,
            &DuckDbReadOptions::default()
        )
        .unwrap(),
        "read_parquet('a''); DROP TABLE observations; --.parquet')"
    );
    let mut options = DuckDbReadOptions {
        header: Some(true),
        delimiter: Some("'".into()),
        timestamp_format: None,
        extra: [("nullstr".into(), json!(["NA", "O'Reilly"]))].into(),
    };
    assert_eq!(
        duckdb_read_options_sql(&options).unwrap(),
        ", header = true, delim = '''', nullstr = ['NA', 'O''Reilly']"
    );
    for (name, value) in [
        ("header", json!(false)),
        ("delim", json!(",")),
        ("timestampformat", json!("%Y")),
        ("x); SELECT 1; --", json!(true)),
        ("", json!(true)),
        ("columns", json!({"value":"INTEGER"})),
        ("nullstr", Value::Null),
        ("nullstr", json!(["NA", null])),
    ] {
        options.extra = [(name.into(), value)].into();
        assert!(duckdb_read_options_sql(&options).is_err(), "{name}");
    }
}
