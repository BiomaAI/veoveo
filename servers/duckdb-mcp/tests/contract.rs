fn check_schema<T: schemars::JsonSchema>(
    current: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
) {
    assert!(
        current
            .insert(
                name.into(),
                serde_json::to_value(schemars::schema_for!(T)).unwrap()
            )
            .is_none(),
        "repeated owner schema {name}"
    );
}
#[path = "contract/catalog.rs"]
mod catalog;
#[path = "contract/execution.rs"]
mod execution;
#[path = "contract/requests.rs"]
mod requests;
#[path = "contract/resources.rs"]
mod resources;
#[path = "contract/results.rs"]
mod results;
#[path = "contract/source_addresses.rs"]
mod source_addresses;
use serde_json::{Value, json};
use veoveo_duckdb_mcp::contract::*;

#[test]
fn schemas_match_the_declared_contract() {
    let baseline: Value =
        serde_json::from_str(include_str!("../testdata/source-contract.schema.json")).unwrap();
    let mut current = serde_json::Map::new();

    check_schema::<DuckDbFormat>(&mut current, stringify!(DuckDbFormat));
    check_schema::<DuckDbReadOptions>(&mut current, stringify!(DuckDbReadOptions));
    check_schema::<DuckDbReadOptionName>(&mut current, stringify!(DuckDbReadOptionName));
    check_schema::<DuckDbReadOptionValue>(&mut current, stringify!(DuckDbReadOptionValue));
    check_schema::<DuckDbReadOptionText>(&mut current, stringify!(DuckDbReadOptionText));
    check_schema::<DuckDbSqlText>(&mut current, stringify!(DuckDbSqlText));
    check_schema::<DuckDbTableName>(&mut current, stringify!(DuckDbTableName));
    check_schema::<DuckDbColumnName>(&mut current, stringify!(DuckDbColumnName));
    check_schema::<DuckDbSource>(&mut current, stringify!(DuckDbSource));
    check_schema::<DuckDbTabularSource>(&mut current, stringify!(DuckDbTabularSource));
    check_schema::<DuckDbSourceUris>(&mut current, stringify!(DuckDbSourceUris));
    check_schema::<DuckDbArtifactSourceUri>(&mut current, stringify!(DuckDbArtifactSourceUri));
    check_schema::<DuckDbDatabaseId>(&mut current, stringify!(DuckDbDatabaseId));
    check_schema::<DuckDbTabularFormat>(&mut current, stringify!(DuckDbTabularFormat));
    check_schema::<DuckDbQueryOutputMode>(&mut current, stringify!(DuckDbQueryOutputMode));
    check_schema::<DuckDbQueryRequest>(&mut current, stringify!(DuckDbQueryRequest));
    check_schema::<DuckDbExecuteRequest>(&mut current, stringify!(DuckDbExecuteRequest));
    check_schema::<DuckDbIngestMode>(&mut current, stringify!(DuckDbIngestMode));
    check_schema::<DuckDbIngestRequest>(&mut current, stringify!(DuckDbIngestRequest));
    check_schema::<DuckDbTabularSelection>(&mut current, stringify!(DuckDbTabularSelection));
    check_schema::<DuckDbExportRequest>(&mut current, stringify!(DuckDbExportRequest));
    check_schema::<DuckDbColumn>(&mut current, stringify!(DuckDbColumn));
    check_schema::<DuckDbQueryOutput>(&mut current, stringify!(DuckDbQueryOutput));
    check_schema::<DuckDbExecuteOutput>(&mut current, stringify!(DuckDbExecuteOutput));
    check_schema::<DuckDbIngestOutput>(&mut current, stringify!(DuckDbIngestOutput));
    check_schema::<DuckDbExportOutput>(&mut current, stringify!(DuckDbExportOutput));
    check_schema::<DuckDbDatabaseUri>(&mut current, stringify!(DuckDbDatabaseUri));
    check_schema::<DuckDbResource>(&mut current, stringify!(DuckDbResource));
    check_schema::<DuckDbDatabaseCursor>(&mut current, stringify!(DuckDbDatabaseCursor));
    check_schema::<DuckDbDatabaseEntry>(&mut current, stringify!(DuckDbDatabaseEntry));
    check_schema::<DuckDbDatabasePage>(&mut current, stringify!(DuckDbDatabasePage));
    check_schema::<DuckDbDatabaseSchema>(&mut current, stringify!(DuckDbDatabaseSchema));
    check_schema::<DuckDbTableSchema>(&mut current, stringify!(DuckDbTableSchema));
    check_schema::<DuckDbSchemaColumn>(&mut current, stringify!(DuckDbSchemaColumn));
    check_schema::<DuckDbArtifactOrigin>(&mut current, stringify!(DuckDbArtifactOrigin));
    check_schema::<DuckDbArtifactOperation>(&mut current, stringify!(DuckDbArtifactOperation));
    check_schema::<DuckDbUsageDetails>(&mut current, stringify!(DuckDbUsageDetails));
    check_schema::<DuckDbQueryUsage>(&mut current, stringify!(DuckDbQueryUsage));
    if let Some(output) = std::env::var_os("VEOVEO_CAPTURE_DUCKDB_SOURCE_SCHEMA") {
        use std::io::Write;
        let output = std::path::PathBuf::from(output);
        assert!(output.is_absolute(), "schema capture path must be absolute");
        let bytes = serde_json::to_vec_pretty(&current).unwrap();
        assert!(
            bytes.len() <= 4 * 1024 * 1024,
            "schema capture exceeds 4 MiB"
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .unwrap();
        file.write_all(&bytes).unwrap();
        file.write_all(b"\n").unwrap();
    }
    assert_eq!(Value::Object(current), baseline);
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
    assert_eq!(*query.output(), DuckDbQueryOutputMode::Inline {});
    assert!(query.attachments().is_empty());
    assert!(query.row_limit().is_none());
    assert!(query.timeout_ms().is_none());
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
        ),
        "read_parquet('a''); DROP TABLE observations; --.parquet')"
    );
    let options = DuckDbReadOptions::default()
        .with_header(true)
        .with_delimiter("'")
        .unwrap()
        .with_extra(
            "nullstr".parse().unwrap(),
            DuckDbReadOptionValue::Array(vec![
                DuckDbReadOptionValue::String("NA".parse().unwrap()),
                DuckDbReadOptionValue::String("O'Reilly".parse().unwrap()),
            ]),
        )
        .unwrap();
    assert_eq!(
        duckdb_read_options_sql(&options),
        ", header = true, delim = '''', nullstr = ['NA', 'O''Reilly']"
    );
}
