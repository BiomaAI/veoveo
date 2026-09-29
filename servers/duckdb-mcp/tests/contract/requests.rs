use serde::Deserialize;
use serde_json::{Value, json};
use veoveo_duckdb_mcp::contract::*;

#[derive(Deserialize)]
struct Case {
    schema: String,
    accepted: bool,
    value: Value,
}

#[test]
fn request_admission_matches_the_shared_schema_cases() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../testdata/request-admission.json")).unwrap();
    for case in cases {
        let admitted = match case.schema.as_str() {
            "DuckDbReadOptions" => {
                serde_json::from_value::<DuckDbReadOptions>(case.value.clone()).is_ok()
            }
            "DuckDbReadOptionValue" => {
                serde_json::from_value::<DuckDbReadOptionValue>(case.value.clone()).is_ok()
            }
            "DuckDbQueryRequest" => {
                serde_json::from_value::<DuckDbQueryRequest>(case.value.clone()).is_ok()
            }
            "DuckDbExecuteRequest" => {
                serde_json::from_value::<DuckDbExecuteRequest>(case.value.clone()).is_ok()
            }
            "DuckDbIngestRequest" => {
                serde_json::from_value::<DuckDbIngestRequest>(case.value.clone()).is_ok()
            }
            "DuckDbExportRequest" => {
                serde_json::from_value::<DuckDbExportRequest>(case.value.clone()).is_ok()
            }
            other => panic!("unrecognized fixture type {other}"),
        };
        assert_eq!(admitted, case.accepted, "{}: {}", case.schema, case.value);
    }
}

#[test]
fn query_builder_checks_relationships_before_exposing_a_request() {
    let base =
        || DuckDbQueryRequest::builder("metrics".parse().unwrap(), "SELECT 1".parse().unwrap());
    let output = || DuckDbQueryOutputMode::Artifact {
        format: DuckDbTabularFormat::Parquet,
    };
    let valid = base()
        .attach(["other".parse().unwrap()])
        .row_limit(10.try_into().unwrap())
        .timeout_ms(200.try_into().unwrap())
        .build()
        .unwrap();
    assert_eq!(valid.database().as_str(), "metrics");
    assert_eq!(valid.attachments()[0].as_str(), "other");
    let wire = serde_json::to_value(&valid).unwrap();
    assert_eq!(
        serde_json::from_value::<DuckDbQueryRequest>(wire).unwrap(),
        valid
    );
    assert_eq!(
        base()
            .attach(["metrics".parse().unwrap()])
            .build()
            .unwrap_err(),
        DuckDbQueryRequestError::MainDatabaseAttachment
    );
    assert_eq!(
        base()
            .attach(["other".parse().unwrap(), "other".parse().unwrap()])
            .build()
            .unwrap_err(),
        DuckDbQueryRequestError::DuplicateAttachment
    );
    assert_eq!(
        base()
            .row_limit(1.try_into().unwrap())
            .output(output())
            .build()
            .unwrap_err(),
        DuckDbQueryRequestError::ArtifactRowLimit
    );
    assert!(base().output(output()).build().is_ok());
    assert!(
        serde_json::from_value::<DuckDbQueryRequest>(
            json!({"db":"metrics", "sql":"SELECT 1", "attach":["metrics"]})
        )
        .is_err()
    );
}

#[test]
fn reader_builder_rejects_duplicates_and_unsafe_text_at_admission() {
    let options = DuckDbReadOptions::default()
        .with_header(true)
        .with_delimiter("\t")
        .unwrap()
        .with_extra(
            "sample_size".parse().unwrap(),
            DuckDbReadOptionValue::Number((-1).into()),
        )
        .unwrap();
    assert!(
        options
            .clone()
            .with_extra(
                "sample_size".parse().unwrap(),
                DuckDbReadOptionValue::Bool(false)
            )
            .is_err()
    );
    assert!(
        serde_json::from_str::<DuckDbReadOptions>(r#"{"extra":{"nullstr":"NA","nullstr":"null"}}"#)
            .is_err()
    );
    assert!(DuckDbReadOptions::default().with_delimiter("\0").is_err());
    assert!(
        DuckDbReadOptions::default()
            .with_timestamp_format("%Y\0")
            .is_err()
    );
    assert!(DuckDbReadOptionText::new("secret\0").is_err());
    assert_eq!(
        duckdb_read_options_sql(&options),
        ", header = true, delim = '\t', sample_size = -1"
    );
}

#[test]
fn table_names_preserve_quoted_identity_in_requests_and_metadata() {
    let name = DuckDbTableName::new("  Order \"Lines\"  ").unwrap();
    assert_eq!(
        duckdb_quote_identifier(name.as_str()),
        "\"  Order \"\"Lines\"\"  \""
    );
    let origin = DuckDbArtifactOrigin::new(
        "metrics".parse().unwrap(),
        DuckDbArtifactOperation::ExportTable {
            table: name.clone(),
            row_count: 1,
        },
    );
    assert_eq!(
        serde_json::to_value(origin).unwrap()["operation"]["table"],
        name.as_str()
    );
    let request = DuckDbIngestRequest::new(
        "metrics".parse().unwrap(),
        name.clone(),
        DuckDbSource::InlineCsv {
            csv: "x\n1\n".into(),
            filename: None,
            options: DuckDbReadOptions::default(),
        },
        DuckDbIngestMode::Create,
    );
    assert_eq!(
        serde_json::to_value(request).unwrap()["table"],
        name.as_str()
    );
    for invalid in ["", " \n\t", "\u{2003}", "a\0b"] {
        assert!(DuckDbTableName::new(invalid).is_err());
        assert!(DuckDbSqlText::new(invalid).is_err());
    }
}
