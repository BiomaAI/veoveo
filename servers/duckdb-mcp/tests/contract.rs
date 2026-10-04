fn check_schema<T: schemars::JsonSchema>(baseline: &serde_json::Value, name: &str) {
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
        baseline[name],
        "{name}"
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

    check_schema::<DuckDbFormat>(&baseline, stringify!(DuckDbFormat));
    check_schema::<DuckDbReadOptions>(&baseline, stringify!(DuckDbReadOptions));
    check_schema::<DuckDbReadOptionName>(&baseline, stringify!(DuckDbReadOptionName));
    check_schema::<DuckDbReadOptionValue>(&baseline, stringify!(DuckDbReadOptionValue));
    check_schema::<DuckDbReadOptionText>(&baseline, stringify!(DuckDbReadOptionText));
    check_schema::<DuckDbSqlText>(&baseline, stringify!(DuckDbSqlText));
    check_schema::<DuckDbTableName>(&baseline, stringify!(DuckDbTableName));
    check_schema::<DuckDbColumnName>(&baseline, stringify!(DuckDbColumnName));
    check_schema::<DuckDbSource>(&baseline, stringify!(DuckDbSource));
    check_schema::<DuckDbTabularSource>(&baseline, stringify!(DuckDbTabularSource));
    check_schema::<DuckDbSourceUris>(&baseline, stringify!(DuckDbSourceUris));
    check_schema::<DuckDbArtifactSourceUri>(&baseline, stringify!(DuckDbArtifactSourceUri));
    check_schema::<DuckDbDatabaseId>(&baseline, stringify!(DuckDbDatabaseId));
    check_schema::<DuckDbTabularFormat>(&baseline, stringify!(DuckDbTabularFormat));
    check_schema::<DuckDbQueryOutputMode>(&baseline, stringify!(DuckDbQueryOutputMode));
    check_schema::<DuckDbQueryRequest>(&baseline, stringify!(DuckDbQueryRequest));
    check_schema::<DuckDbExecuteRequest>(&baseline, stringify!(DuckDbExecuteRequest));
    check_schema::<DuckDbIngestMode>(&baseline, stringify!(DuckDbIngestMode));
    check_schema::<DuckDbIngestRequest>(&baseline, stringify!(DuckDbIngestRequest));
    check_schema::<DuckDbTabularSelection>(&baseline, stringify!(DuckDbTabularSelection));
    check_schema::<DuckDbExportRequest>(&baseline, stringify!(DuckDbExportRequest));
    check_schema::<DuckDbColumn>(&baseline, stringify!(DuckDbColumn));
    check_schema::<DuckDbQueryOutput>(&baseline, stringify!(DuckDbQueryOutput));
    check_schema::<DuckDbExecuteOutput>(&baseline, stringify!(DuckDbExecuteOutput));
    check_schema::<DuckDbIngestOutput>(&baseline, stringify!(DuckDbIngestOutput));
    check_schema::<DuckDbExportOutput>(&baseline, stringify!(DuckDbExportOutput));
    check_schema::<DuckDbDatabaseUri>(&baseline, stringify!(DuckDbDatabaseUri));
    check_schema::<DuckDbResource>(&baseline, stringify!(DuckDbResource));
    check_schema::<DuckDbDatabaseCursor>(&baseline, stringify!(DuckDbDatabaseCursor));
    check_schema::<DuckDbDatabaseEntry>(&baseline, stringify!(DuckDbDatabaseEntry));
    check_schema::<DuckDbDatabasePage>(&baseline, stringify!(DuckDbDatabasePage));
    check_schema::<DuckDbDatabaseSchema>(&baseline, stringify!(DuckDbDatabaseSchema));
    check_schema::<DuckDbTableSchema>(&baseline, stringify!(DuckDbTableSchema));
    check_schema::<DuckDbSchemaColumn>(&baseline, stringify!(DuckDbSchemaColumn));
    check_schema::<DuckDbArtifactOrigin>(&baseline, stringify!(DuckDbArtifactOrigin));
    check_schema::<DuckDbArtifactOperation>(&baseline, stringify!(DuckDbArtifactOperation));
    check_schema::<DuckDbUsageDetails>(&baseline, stringify!(DuckDbUsageDetails));
    check_schema::<DuckDbQueryUsage>(&baseline, stringify!(DuckDbQueryUsage));
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
