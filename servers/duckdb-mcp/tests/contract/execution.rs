use serde_json::json;
use veoveo_duckdb_mcp::contract::*;
use veoveo_types::{TaskId, TaskTypeDefinition};

#[test]
fn artifact_origins_separate_direct_calls_from_native_tasks() {
    let task = TaskId::new();
    for operation in [
        DuckDbArtifactOperation::Query { row_count: 0 },
        DuckDbArtifactOperation::ExportSql { row_count: 12 },
        DuckDbArtifactOperation::ExportTable {
            table: "observations".into(),
            row_count: 12,
        },
        DuckDbArtifactOperation::Snapshot {},
    ] {
        let direct = DuckDbArtifactOrigin::new("metrics".parse().unwrap(), operation).unwrap();
        let wire = serde_json::to_value(&direct).unwrap();
        assert!(wire.get("task_id").is_none());
        assert_eq!(
            serde_json::from_value::<DuckDbArtifactOrigin>(wire).unwrap(),
            direct
        );
        let durable = direct.with_task(task).unwrap();
        let wire = serde_json::to_value(&durable).unwrap();
        assert_eq!(wire["task_id"], task.to_string());
        assert_eq!(
            serde_json::from_value::<DuckDbArtifactOrigin>(wire).unwrap(),
            durable
        );
        assert_eq!(durable.database().as_str(), "metrics");
    }
}

#[test]
fn artifact_origin_admission_rejects_fake_tasks_and_invalid_operation_facts() {
    let valid = json!({"db":"metrics","operation":{"kind":"query","row_count":1}});
    for task in [
        "call-01983da0-0000-7000-8000-000000000001",
        "01983da0-0000-4000-8000-000000000001",
        "01983da0-0000-7000-0000-000000000001",
    ] {
        let mut wire = valid.clone();
        wire["task_id"] = json!(task);
        assert!(serde_json::from_value::<DuckDbArtifactOrigin>(wire).is_err());
    }
    for operation in [
        json!({"kind":"export_table","table":"  ","row_count":1}),
        json!({"kind":"snapshot","row_count":1}),
        json!({"kind":"export_sql"}),
        json!({"kind":"unknown"}),
    ] {
        let mut wire = valid.clone();
        wire["operation"] = operation;
        assert!(serde_json::from_value::<DuckDbArtifactOrigin>(wire).is_err());
    }
    assert!(
        DuckDbArtifactOrigin::new(
            "metrics".parse().unwrap(),
            DuckDbArtifactOperation::ExportTable {
                table: " ".into(),
                row_count: 1
            }
        )
        .is_err()
    );
}

#[test]
fn usage_details_derive_the_operation_and_preserve_domain_ids() {
    for (details, operation) in [
        (
            DuckDbUsageDetails::Query {
                result: DuckDbQueryUsage::Inline {
                    rows_returned: 1,
                    truncated: true,
                },
            },
            "query",
        ),
        (
            DuckDbUsageDetails::Execute {
                db: "metrics".parse().unwrap(),
                statements: 1,
            },
            "execute",
        ),
        (
            DuckDbUsageDetails::Ingest {
                db: "metrics".parse().unwrap(),
                table: "observations".into(),
            },
            "ingest",
        ),
        (
            DuckDbUsageDetails::Export {
                db: "metrics".parse().unwrap(),
                artifact: "01983da0-0000-7000-8000-000000000001".parse().unwrap(),
            },
            "export",
        ),
    ] {
        assert_eq!(details.operation().name().as_str(), operation);
        let wire = serde_json::to_value(&details).unwrap();
        assert_eq!(wire["operation"], operation);
        assert_eq!(
            serde_json::from_value::<DuckDbUsageDetails>(wire).unwrap(),
            details
        );
    }
    assert!(
        serde_json::from_value::<DuckDbUsageDetails>(
            json!({"operation":"export","db":"../other","artifact":"bad"})
        )
        .is_err()
    );
}
