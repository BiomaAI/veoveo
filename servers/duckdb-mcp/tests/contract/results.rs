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
fn result_and_export_admission_matches_the_declared_cases() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../testdata/result-admission.json")).unwrap();
    for case in cases {
        let admitted = match case.schema.as_str() {
            "DuckDbExportRequest" => {
                serde_json::from_value::<DuckDbExportRequest>(case.value.clone()).is_ok()
            }
            "DuckDbQueryOutputMode" => {
                serde_json::from_value::<DuckDbQueryOutputMode>(case.value.clone()).is_ok()
            }
            "DuckDbQueryOutput" => {
                serde_json::from_value::<DuckDbQueryOutput>(case.value.clone()).is_ok()
            }
            other => panic!("unrecognized fixture type {other}"),
        };
        assert_eq!(admitted, case.accepted, "{}: {}", case.schema, case.value);
    }
}

#[test]
fn typed_export_requests_keep_the_single_wire_format() {
    let db = "metrics".parse::<DuckDbDatabaseId>().unwrap();
    for format in [DuckDbTabularFormat::Csv, DuckDbTabularFormat::Parquet] {
        for selection in [
            DuckDbTabularSelection::Table {
                table: "facts".parse().unwrap(),
            },
            DuckDbTabularSelection::Sql {
                sql: "SELECT 42".parse().unwrap(),
            },
        ] {
            let request = DuckDbExportRequest::Tabular {
                db: db.clone(),
                selection,
                format,
            };
            assert_eq!(request.database(), &db);
            let wire = serde_json::to_value(&request).unwrap();
            assert!(wire.get("selection").is_some());
            assert_eq!(wire["format"], serde_json::to_value(format).unwrap());
            assert_eq!(
                serde_json::from_value::<DuckDbExportRequest>(wire).unwrap(),
                request
            );
        }
    }
    let snapshot = DuckDbExportRequest::Snapshot { db };
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap(),
        json!({
            "db":"metrics","format":"duck_db","selection":{"kind":"database"}
        })
    );
    assert_eq!(
        serde_json::from_value::<DuckDbExportRequest>(serde_json::to_value(&snapshot).unwrap())
            .unwrap(),
        snapshot
    );
}

#[test]
fn query_output_checks_row_shape_and_observed_count() {
    let column = DuckDbColumn {
        name: "answer".into(),
        type_name: "BIGINT".into(),
    };
    for (rows, count, truncated) in [
        (vec![vec![json!(42)]], 1, false),
        (vec![vec![json!(42)]], 2, true),
        (vec![], 1, true),
        (vec![], 0, false),
    ] {
        let output =
            DuckDbQueryOutput::inline(vec![column.clone()], rows.clone(), count, truncated)
                .unwrap();
        assert_eq!(output.columns(), std::slice::from_ref(&column));
        assert_eq!(output.rows(), rows);
        assert_eq!(output.row_count(), count);
        assert_eq!(output.truncated(), truncated);
        assert!(output.artifact().is_none());
        assert_eq!(
            serde_json::from_value::<DuckDbQueryOutput>(serde_json::to_value(&output).unwrap())
                .unwrap(),
            output
        );
    }
    for (rows, count, truncated) in [
        (vec![vec![json!(42), json!(7)]], 1, false),
        (vec![vec![]], 1, false),
        (vec![vec![json!(42)]], 0, false),
        (vec![vec![json!(42)]], 2, false),
        (vec![vec![json!(42)]], 1, true),
        (vec![], 0, true),
    ] {
        assert!(
            DuckDbQueryOutput::inline(vec![column.clone()], rows.clone(), count, truncated)
                .is_err()
        );
        assert!(
            serde_json::from_value::<DuckDbQueryOutput>(json!({
                "columns":[column],"rows":rows,"rowCount":count,"truncated":truncated
            }))
            .is_err()
        );
    }
}

#[test]
fn export_product_address_and_closed_wire_are_admitted_together() {
    let id = veoveo_artifact_contract::ArtifactId::new();
    let artifact: veoveo_artifact_contract::ArtifactMetadata = serde_json::from_value(json!({
        "artifactId":id,"artifactUri":id.plane_uri(),"byteLen":1,"createdAt":"2026-09-29T00:00:00Z"
    }))
    .unwrap();
    let output = DuckDbExportOutput::new("metrics".parse().unwrap(), 1, artifact);
    let wire = serde_json::to_value(output).unwrap();
    assert_eq!(wire["resultUri"], wire["artifact"]["artifactUri"]);
    assert!(serde_json::from_value::<DuckDbExportOutput>(wire.clone()).is_ok());
    for field in ["missing", "null", "mismatch", "unknown"] {
        let mut value = wire.clone();
        match field {
            "missing" => {
                value.as_object_mut().unwrap().remove("resultUri");
            }
            "null" => value["resultUri"] = Value::Null,
            "mismatch" => {
                value["resultUri"] = json!(veoveo_artifact_contract::ArtifactId::new().plane_uri())
            }
            _ => value["unknown"] = json!(true),
        }
        assert!(
            serde_json::from_value::<DuckDbExportOutput>(value).is_err(),
            "{field}"
        );
    }
}
