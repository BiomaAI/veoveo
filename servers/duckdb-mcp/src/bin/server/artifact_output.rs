//! Checked direct or Task-backed Artifact publication.
use super::app_state::AppState;
use rmcp::ErrorData as McpError;
use std::collections::BTreeSet;
use veoveo_artifact_contract::{ArtifactMetadata, ArtifactPut, ComplianceMetadata};
use veoveo_artifact_contract::{ArtifactWriteIdempotencyKey, IssuedArtifactWriteCapability};
use veoveo_duckdb_mcp::contract::{DuckDbArtifactOrigin, DuckDbTaskKind};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_types::{DataLabelId, TaskId, TaskTypeDefinition};

pub(super) struct ArtifactWriter(Writer);
enum Writer {
    Caller(Box<PlaneCaller>),
    Task {
        capability: IssuedArtifactWriteCapability,
        idempotency_key: ArtifactWriteIdempotencyKey,
        task_id: TaskId,
        operation: DuckDbTaskKind,
        data_labels: BTreeSet<DataLabelId>,
    },
}
impl ArtifactWriter {
    pub(super) fn caller(caller: PlaneCaller) -> Self {
        Self(Writer::Caller(Box::new(caller)))
    }
    pub(super) fn for_task(
        capability: Option<&IssuedArtifactWriteCapability>,
        task_id: TaskId,
        operation: DuckDbTaskKind,
        identity: &GatewayInternalIdentity,
    ) -> Result<Self, String> {
        let capability = capability.ok_or("Task did not reserve artifact write capability")?;
        let idempotency_key = task_write_key(capability, task_id, operation)?;
        Ok(Self(Writer::Task {
            capability: capability.clone(),
            idempotency_key,
            task_id,
            operation,
            data_labels: identity.actor.data_labels.clone(),
        }))
    }

    fn prepare(
        &self,
        mut put: ArtifactPut,
        origin: DuckDbArtifactOrigin,
    ) -> Result<ArtifactPut, McpError> {
        let (task_id, operation, data_labels) = match &self.0 {
            Writer::Caller(caller) => (
                None,
                DuckDbTaskKind::Query,
                &caller.identity.actor.data_labels,
            ),
            Writer::Task {
                task_id,
                operation,
                data_labels,
                ..
            } => (Some(*task_id), *operation, data_labels),
        };
        if origin.operation().task_kind() != operation {
            return Err(McpError::invalid_params(
                "artifact origin does not match the publishing operation",
                None,
            ));
        }
        // The publication authority supplies the Task association; a caller's
        // metadata cannot substitute an unrelated Task or invent one for a direct call.
        let origin = match (task_id, origin.task_id()) {
            (Some(expected), Some(actual)) if expected != actual => {
                return Err(McpError::invalid_params(
                    "artifact origin belongs to another Task",
                    None,
                ));
            }
            (None, Some(_)) => {
                return Err(McpError::invalid_params(
                    "a direct call cannot assert a Task origin",
                    None,
                ));
            }
            (Some(task_id), _) => origin
                .with_task(task_id)
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?,
            (None, None) => origin,
        };
        put.compliance = ComplianceMetadata {
            data_labels: data_labels.clone(),
            ..Default::default()
        };
        put.metadata = serde_json::to_value(origin).map_err(|_| {
            McpError::internal_error("serializing DuckDB artifact origin failed", None)
        })?;
        Ok(put)
    }
}

fn task_write_key(
    capability: &IssuedArtifactWriteCapability,
    task_id: TaskId,
    operation: DuckDbTaskKind,
) -> Result<ArtifactWriteIdempotencyKey, String> {
    if !matches!(operation, DuckDbTaskKind::Query | DuckDbTaskKind::Export) {
        return Err("DuckDB operation does not publish artifacts".into());
    }
    if capability.task_id.as_uuid() != task_id.as_uuid() {
        return Err("artifact write capability belongs to another Task".into());
    }
    veoveo_duckdb_mcp::contract::DuckDbTaskUsageUri::new(task_id)
        .map_err(|error| error.to_string())?;
    ArtifactWriteIdempotencyKey::new(format!("duckdb:{task_id}:{}", operation.name()))
        .map_err(|error| error.to_string())
}

pub(super) async fn put_op_artifact(
    state: &AppState,
    writer: &ArtifactWriter,
    bytes: Vec<u8>,
    mime_type: &str,
    filename: String,
    origin: DuckDbArtifactOrigin,
) -> Result<ArtifactMetadata, McpError> {
    let mut put = ArtifactPut::new(bytes);
    put.mime_type = Some(mime_type.into());
    put.filename = Some(filename);
    let put = writer.prepare(put, origin)?;
    let result = match &writer.0 {
        Writer::Caller(caller) => state.artifacts.put(caller, put).await,
        Writer::Task {
            capability,
            idempotency_key,
            ..
        } => {
            state
                .artifacts
                .put_with_capability(capability, idempotency_key.clone(), put)
                .await
        }
    };
    result.map_err(|error| {
        McpError::internal_error(format!("artifact write failed: {error:#}"), None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_artifact_contract::{ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret};
    use veoveo_duckdb_mcp::contract::DuckDbArtifactOperation;

    fn capability(task_id: TaskId) -> IssuedArtifactWriteCapability {
        IssuedArtifactWriteCapability {
            capability_id: ArtifactWriteCapabilityId::new(),
            secret: ArtifactWriteCapabilitySecret::new("a".repeat(32)).unwrap(),
            task_id: veoveo_artifact_contract::ArtifactTaskId::try_from(task_id.as_uuid()).unwrap(),
            expires_at: chrono::Utc::now() + chrono::TimeDelta::hours(1),
        }
    }
    fn origin() -> DuckDbArtifactOrigin {
        DuckDbArtifactOrigin::new(
            "metrics".parse().unwrap(),
            DuckDbArtifactOperation::Query { row_count: 3 },
        )
    }

    #[test]
    fn direct_outputs_keep_labels_and_cannot_claim_a_task() {
        let mut identity = crate::test_support::identity("operator", "caller");
        identity
            .actor
            .data_labels
            .insert(DataLabelId::parse("mission").unwrap());
        let caller = veoveo_mcp_contract::PlaneCaller::from_gateway(
            identity.clone(),
            veoveo_mcp_contract::hosting::ForwardedBearer::new("fixture-bearer"),
        );
        let writer = ArtifactWriter::caller(caller);
        let output = writer
            .prepare(ArtifactPut::new(vec![1, 2, 3]), origin())
            .unwrap();
        assert_eq!(output.compliance.data_labels, identity.actor.data_labels);
        assert!(output.metadata.get("taskId").is_none());
        assert_eq!(
            serde_json::from_value::<DuckDbArtifactOrigin>(output.metadata).unwrap(),
            origin()
        );
        assert!(
            writer
                .prepare(
                    ArtifactPut::new(vec![]),
                    origin().with_task(TaskId::new()).unwrap()
                )
                .is_err()
        );
        let snapshot = DuckDbArtifactOrigin::new(
            "metrics".parse().unwrap(),
            DuckDbArtifactOperation::Snapshot {},
        );
        assert!(writer.prepare(ArtifactPut::new(vec![]), snapshot).is_err());
    }

    #[test]
    fn task_outputs_bind_capability_origin_operation_and_labels() {
        let mut identity = crate::test_support::identity("operator", "caller");
        identity
            .actor
            .data_labels
            .insert(DataLabelId::parse("mission").unwrap());
        let task_id = TaskId::new();
        let capability = capability(task_id);
        let writer =
            ArtifactWriter::for_task(Some(&capability), task_id, DuckDbTaskKind::Query, &identity)
                .unwrap();
        let output = writer.prepare(ArtifactPut::new(vec![]), origin()).unwrap();
        assert_eq!(output.compliance.data_labels, identity.actor.data_labels);
        assert_eq!(
            serde_json::from_value::<DuckDbArtifactOrigin>(output.metadata)
                .unwrap()
                .task_id(),
            Some(task_id)
        );
        assert!(
            writer
                .prepare(
                    ArtifactPut::new(vec![]),
                    origin().with_task(TaskId::new()).unwrap()
                )
                .is_err()
        );
        assert!(
            ArtifactWriter::for_task(
                Some(&capability),
                TaskId::new(),
                DuckDbTaskKind::Query,
                &identity
            )
            .is_err()
        );
        assert!(ArtifactWriter::for_task(None, task_id, DuckDbTaskKind::Query, &identity).is_err());
        for operation in [DuckDbTaskKind::Execute, DuckDbTaskKind::Ingest] {
            assert!(
                ArtifactWriter::for_task(Some(&capability), task_id, operation, &identity).is_err()
            );
        }
        assert_eq!(
            task_write_key(&capability, task_id, DuckDbTaskKind::Query)
                .unwrap()
                .as_str(),
            format!("duckdb:{task_id}:query")
        );
    }
}
