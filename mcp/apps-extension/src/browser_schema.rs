//! Browser schemas for the pinned final MCP Task protocol declarations.
use schemars::JsonSchema;
use serde_json::{Value, json};
#[derive(JsonSchema)]
#[allow(dead_code)]
struct TaskContracts {
    seed: rmcp::model::CreateTaskResult,
    detail: rmcp::model::GetTaskResult,
}
/// Preserve SDK field declarations and qualify custom decoder relationships.
pub fn task_schema_bundle() -> Value {
    let mut schema =
        serde_json::to_value(schemars::schema_for!(TaskContracts)).expect("Task schema serializes");
    schema["$defs"]["CreateTaskResult"]["properties"]["resultType"] = json!({"const":"task"});
    let relationships = json!([
        {"if":{"properties":{"status":{"const":"completed"}},"required":["status"]},"then":{"required":["result"],"properties":{"result":{"type":"object"}}}},
        {"if":{"properties":{"status":{"const":"failed"}},"required":["status"]},"then":{"required":["error"],"properties":{"error":{"type":"object"}}}},
        {"if":{"properties":{"status":{"const":"input_required"}},"required":["status"]},"then":{"required":["inputRequests"],"properties":{"inputRequests":{"type":"object"}}}}
    ]);
    let all_of = schema["$defs"]["GetTaskResult"]
        .as_object_mut()
        .expect("SDK Task result schema is an object")
        .entry("allOf")
        .or_insert_with(|| json!([]));
    all_of
        .as_array_mut()
        .expect("schema allOf is an array")
        .extend(relationships.as_array().unwrap().iter().cloned());
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_task_serialization_agrees_with_browser_fixture() {
        use rmcp::model::{CreateTaskResult, DetailedTask, GetTaskResult, Task, TaskPayload};
        let mut task = Task::default();
        task.task_id = "0195dabe-7777-7abc-8def-000000000001".into();
        task.created_at = "2026-10-01T00:00:00Z".into();
        task.last_updated_at = task.created_at.clone();
        task.poll_interval_ms = Some(10);
        let result = json!({"resultType":"complete","content":[{"type":"text","text":"ready"}],"structuredContent":{"provider":{"open":null}}});
        let wire = json!({
            "seed": CreateTaskResult::new(task.clone()),
            "detail": GetTaskResult::new(DetailedTask::new(task.clone(), TaskPayload::Working)),
            "complete": GetTaskResult::new(DetailedTask::new(task, TaskPayload::Completed { result: serde_json::from_value(result).unwrap() }))
        });
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/final-tasks.json")).unwrap();
        assert_eq!(wire, fixture);
    }
}
