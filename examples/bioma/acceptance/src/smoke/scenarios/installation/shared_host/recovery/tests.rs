use super::*;
fn input() -> Result<Input> {
    Ok(Input {
        request: DuckDbQueryRequest::builder(
            "metrics".parse()?,
            "SELECT sum(value) AS answer FROM immutable_facts".parse()?,
        )
        .row_limit(NonZeroU64::new(10).unwrap())
        .timeout_ms(NonZeroU64::new(30_000).unwrap())
        .build()?,
        expected: Expected {
            columns: vec![DuckDbColumn {
                name: "answer".into(),
                type_name: "INTEGER".into(),
            }],
            rows: vec![vec![serde_json::json!(7)]],
        },
        deadline_seconds: 420,
        reserved_work_context: veoveo_types::WorkContextId::parse("fixture-context")?,
    })
}
#[test]
fn working_query_fixture_requires_selected_database_semantics_and_bounded_budget() -> Result<()> {
    let selected = "metrics".parse()?;
    let mut input = input()?;
    input.admit(&selected)?;
    assert!(input.admit(&"other".parse()?).is_err());
    input.deadline_seconds = 601;
    assert!(input.admit(&selected).is_err());
    input.deadline_seconds = 420;
    input.expected.rows[0].clear();
    assert!(input.admit(&selected).is_err());
    let mut wire = serde_json::json!({"request":{"db":"metrics","sql":"SELECT 1","rowLimit":1,"timeoutMs":1000},"expected":{"columns":[{"name":"answer","typeName":"INTEGER"}],"rows":[[1]]},"deadlineSeconds":420,"reservedWorkContext":"fixture-context"});
    assert!(serde_json::from_value::<Input>(wire.clone()).is_ok());
    wire["execute"] = serde_json::json!({"sql":"DELETE FROM facts"});
    assert!(serde_json::from_value::<Input>(wire).is_err());
    Ok(())
}
#[test]
fn working_query_semantic_subset_rejects_changed_rows_types_counts_and_truncation() -> Result<()> {
    let input = input()?;
    let output = DuckDbQueryOutput::inline(
        vec![
            DuckDbColumn {
                name: "unselected".into(),
                type_name: "INTEGER".into(),
            },
            DuckDbColumn {
                name: "answer".into(),
                type_name: "INTEGER".into(),
            },
        ],
        vec![vec![serde_json::json!(99), serde_json::json!(7)]],
        1,
        false,
    )?;
    let make = |output| CallToolResult::structured(serde_json::to_value(output).unwrap());
    input.require_output(&make(output))?;
    for (answer, ty, count, truncated) in [
        (8, "INTEGER", 1, false),
        (7, "BIGINT", 1, false),
        (7, "INTEGER", 2, true),
    ] {
        let changed = DuckDbQueryOutput::inline(
            vec![DuckDbColumn {
                name: "answer".into(),
                type_name: ty.into(),
            }],
            vec![vec![serde_json::json!(answer)]],
            count,
            truncated,
        )?;
        assert!(input.require_output(&make(changed)).is_err());
    }
    Ok(())
}
#[test]
fn working_recovery_rejects_early_completion_and_changed_identity_but_allows_completion_hints()
-> Result<()> {
    let created = Task::new(
        "opaque-gateway-task",
        TaskStatus::Working,
        "2026-10-09T00:00:00Z",
        "2026-10-09T00:00:00Z",
    );
    let original = identity(&created)?;
    let observed = DetailedTask::new(created.clone(), TaskPayload::Working);
    working(&observed, &original)?;
    let mut foreign = observed.clone();
    foreign.task.task_id = "foreign-task".into();
    assert!(working(&foreign, &original).is_err());
    foreign = observed.clone();
    foreign.task.created_at = "2026-10-09T00:00:01Z".into();
    assert!(working(&foreign, &original).is_err());
    let Value::Object(result) =
        serde_json::to_value(CallToolResult::structured(serde_json::json!({"answer":7})))?
    else {
        unreachable!()
    };
    let done = DetailedTask::new(created, TaskPayload::Completed { result });
    assert!(working(&done, &original).is_err());
    let mut current = done.clone();
    current.task.ttl_ms = Some(5000);
    current.task.poll_interval_ms = Some(4000);
    completed(&done, &current, &original)?;
    current.payload = TaskPayload::Cancelled;
    assert!(completed(&done, &current, &original).is_err());
    Ok(())
}
