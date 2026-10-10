//! Stable completed capture agreement across delivery, current reads and replacement.
use anyhow::{Result, ensure};
use rmcp::model::{CallToolResult, DetailedTask, Task, TaskPayload};
use veoveo_types::CanonicalTaskId;

pub(super) fn completed(
    created: &Task,
    expected: &DetailedTask,
    observed: &DetailedTask,
    id: &CanonicalTaskId,
) -> Result<CallToolResult> {
    for task in [created, &expected.task, &observed.task] {
        ensure!(
            task.task_id == id.as_str() && task.created_at == created.created_at,
            "View completed Task identity/creation time differs"
        );
    }
    ensure!(
        expected.payload == observed.payload,
        "View delivered/current completed payload differs"
    );
    let TaskPayload::Completed { result } = &observed.payload else {
        anyhow::bail!("View Task baseline is not Completed");
    };
    let result: CallToolResult = serde_json::from_value(serde_json::Value::Object(result.clone()))
        .map_err(|_| anyhow::anyhow!("View completed payload failed tool result admission"))?;
    ensure!(
        result.is_error != Some(true),
        "View completed capture returned a tool error"
    );
    Ok(result)
}

pub(super) fn payload_agreement(
    delivered: &CallToolResult,
    current: &CallToolResult,
) -> Result<()> {
    ensure!(
        serde_json::to_value(delivered)? == serde_json::to_value(current)?,
        "View delivered/current tool result differs"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::TaskStatus;
    #[test]
    fn view_completed_delivery_requires_stable_identity_and_payload_allows_hints() -> Result<()> {
        let id = CanonicalTaskId::parse("gateway-view-capture")?;
        let created = Task::new(
            id.as_str(),
            TaskStatus::Working,
            "2026-10-09T00:00:00Z",
            "2026-10-09T00:00:00Z",
        );
        let result = CallToolResult::structured(serde_json::json!({"frame":"owner-frame"}));
        let serde_json::Value::Object(body) = serde_json::to_value(&result)? else {
            unreachable!()
        };
        let expected = DetailedTask::new(created.clone(), TaskPayload::Completed { result: body });
        let mut current = expected.clone();
        current.task.ttl_ms = Some(1000);
        current.task.poll_interval_ms = Some(50);
        current.task.last_updated_at = "2026-10-09T00:00:01Z".into();
        let delivered = completed(&created, &expected, &current, &id)?;
        payload_agreement(&delivered, &result)?;
        current.task.task_id = "gateway-other-task".into();
        assert!(completed(&created, &expected, &current, &id).is_err());
        current = expected.clone();
        current.task.created_at = "2026-10-09T00:00:02Z".into();
        assert!(completed(&created, &expected, &current, &id).is_err());
        current = expected.clone();
        let TaskPayload::Completed { result } = &mut current.payload else {
            unreachable!()
        };
        result.insert(
            "structuredContent".into(),
            serde_json::json!({"frame":"another-frame"}),
        );
        assert!(completed(&created, &expected, &current, &id).is_err());
        current = DetailedTask::new(created.clone(), TaskPayload::Cancelled);
        assert!(completed(&created, &expected, &current, &id).is_err());
        assert!(completed(&created, &current, &current, &id).is_err());
        assert!(
            payload_agreement(
                &delivered,
                &CallToolResult::structured(serde_json::json!({"frame":"another-frame"}))
            )
            .is_err()
        );
        Ok(())
    }
}
