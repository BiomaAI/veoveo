//! Typed owner lookup values joined to kernel Task creation and terminal CAS.
use crate::contract::{PipelineId, ReasonTaskKind};
use chrono::{DateTime, Utc};
use surrealdb::types::{Error, Kind, Object, RecordId, SurrealValue, Value};
use veoveo_modules::TableName;
use veoveo_task_runtime::{
    OwnedTaskTable, TaskContribution, TaskContributions, TaskCreation, TaskError, TaskRuntime,
    TaskSettlement, TaskSnapshot,
};
use veoveo_types::{TaskTypeDefinition, TaskTypeName};

#[derive(Clone, SurrealValue)]
pub(crate) struct Identity {
    #[surreal(wrap)]
    pub(crate) pipeline_id: PipelineId,
    pub(crate) tenant: RecordId,
}
fn identity(
    owner: &veoveo_task_runtime::TaskOwner,
    value: &serde_json::Value,
) -> Result<Identity, TaskError> {
    let request: crate::task_request::DurableReasonRequest = serde_json::from_value(value.clone())?;
    let crate::task_request::ReasonTaskInput::Analyze(input) = request.input;
    Ok(Identity {
        pipeline_id: input.pipeline_id,
        tenant: veoveo_platform_store::deterministic_tenant_id(owner.tenant_key())?.record_id(),
    })
}
fn native_json(value: serde_json::Value) -> Value {
    let Value::Object(mut wrapper) =
        veoveo_platform_store::TaskResultRecord::new(value).into_value()
    else {
        unreachable!("typed result envelope")
    };
    wrapper.remove("payload").expect("typed result payload")
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(crate) enum Terminal {
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(crate) enum Outcome {
    #[vocabulary(rename = "product")]
    Product,
    #[vocabulary(rename = "tool_error")]
    ToolError,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(SurrealValue)]
pub(crate) struct Settlement {
    #[surreal(wrap)]
    pub(crate) outcome: Outcome,
    #[surreal(wrap)]
    pub(crate) status: Terminal,
    pub(crate) completed_at: DateTime<Utc>,
    pub(crate) results: Option<RecordId>,
    pub(crate) annotations: Option<RecordId>,
    pub(crate) source_clip: Option<RecordId>,
    pub(crate) expected_result: Option<Value>,
    pub(crate) expected_metadata: Option<MetadataRecord>,
    pub(crate) finding: Option<FindingRecord>,
}
pub(crate) struct MetadataRecord(pub(crate) crate::contract::ReasonArtifactMetadata);
pub(crate) struct FindingRecord(pub(crate) crate::contract::FindingData);
fn from_native<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, Error> {
    let mut wrapper = Object::new();
    wrapper.insert("payload", value);
    serde_json::from_value(
        veoveo_platform_store::TaskResultRecord::from_value(Value::Object(wrapper))?.into_payload(),
    )
    .map_err(|_| Error::internal("invalid Reason lookup value".into()))
}
impl SurrealValue for MetadataRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native_json(serde_json::to_value(self.0).expect("checked Reason value"))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        from_native(value).map(Self)
    }
}
impl SurrealValue for FindingRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native_json(serde_json::to_value(self.0).expect("checked Reason value"))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        from_native(value).map(Self)
    }
}
#[derive(SurrealValue)]
pub(crate) struct FindingRow {
    pub(crate) id: RecordId,
    pub(crate) task: RecordId,
    #[surreal(wrap)]
    pub(crate) task_type: TaskTypeName,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) identity: Identity,
    pub(crate) settlement: Settlement,
    pub(crate) lifecycle: Lifecycle,
}
#[derive(SurrealValue)]
pub(crate) struct Lifecycle {
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    #[surreal(wrap)]
    pub(crate) task_type: TaskTypeName,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) completed_at: Option<DateTime<Utc>>,
    pub(crate) retention_expires_at: Option<DateTime<Utc>>,
    pub(crate) result_matches: bool,
}
struct Contributions {
    table: OwnedTaskTable,
    kinds: Vec<TaskTypeName>,
}
pub fn bind(runtime: TaskRuntime) -> Result<TaskRuntime, TaskError> {
    let table = OwnedTaskTable::new(
        &crate::schema::ownership().map_err(|e| TaskError::InvalidRecord(e.to_string()))?,
        TableName::new("reason_analysis").map_err(|e| TaskError::InvalidRecord(e.to_string()))?,
    )?;
    let kinds = vec![ReasonTaskKind::AnalyzeRecording.name()];
    runtime
        .requiring_contributions(kinds.clone())?
        .bind_contributions(std::sync::Arc::new(Contributions { table, kinds }))
}
impl TaskContributions for Contributions {
    fn table(&self) -> &OwnedTaskTable {
        &self.table
    }
    fn task_types(&self) -> &[TaskTypeName] {
        &self.kinds
    }
    fn created(&self, creation: TaskCreation<'_>) -> Result<TaskContribution, TaskError> {
        TaskContribution::create(
            self.table.clone(),
            identity(&creation.draft.owner, &creation.draft.request)?,
        )
    }
    fn settled(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        let identity = identity(&current.owner, &current.request)?;
        let mut values = Settlement {
            outcome: Outcome::Product,
            status: Terminal::Succeeded,
            completed_at: at,
            results: None,
            annotations: None,
            source_clip: None,
            expected_result: None,
            expected_metadata: None,
            finding: None,
        };
        let result = match settlement {
            TaskSettlement::Succeeded { result } => result,
            TaskSettlement::Failed { .. } => {
                values.status = Terminal::Failed;
                values.outcome = Outcome::Failed;
                return TaskContribution::settle(self.table.clone(), identity, values);
            }
            TaskSettlement::Cancelled => {
                values.status = Terminal::Cancelled;
                values.outcome = Outcome::Cancelled;
                return TaskContribution::settle(self.table.clone(), identity, values);
            }
        };
        let envelope: rmcp::model::CallToolResult = serde_json::from_value(result.clone())?;
        let Some(output) = crate::task_product::validate(&envelope)
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?
        else {
            values.outcome = Outcome::ToolError;
            return TaskContribution::settle(self.table.clone(), identity, values);
        };
        if output.analysis_id().task_id() != current.task_id
            || *output.pipeline_uri.id() != identity.pipeline_id
        {
            return Err(TaskError::InvalidRecord(
                "owner settlement differs from its Task or pipeline".into(),
            ));
        }
        let link = |id: veoveo_artifact_contract::ArtifactId| {
            RecordId::new(
                "artifact_occurrence",
                surrealdb::types::Uuid::from(id.as_uuid()),
            )
        };
        values.results = Some(link(output.results_artifact.artifact_id()));
        values.annotations = Some(link(output.annotations_artifact.artifact_id()));
        values.source_clip = output
            .source_clip_artifact
            .as_ref()
            .map(|a| link(a.artifact_id()));

        let metadata: crate::contract::ReasonArtifactMetadata =
            serde_json::from_value(output.results_artifact.metadata.clone())?;
        match &metadata.provenance {
            crate::contract::ReasonArtifactProvenance::Results {
                analysis_id,
                pipeline_id,
                model_id,
                recording_id,
                prompt_revision,
                task_kind,
                source_snapshot_sha256,
            } if analysis_id.task_id() == current.task_id
                && *pipeline_id == identity.pipeline_id
                && model_id == output.finding.model_id()
                && *recording_id == output.finding.recording_uri().id()
                && prompt_revision == output.finding.prompt_revision()
                && *task_kind == crate::contract::ReasoningKind::from(output.finding.task())
                && source_snapshot_sha256 == output.finding.source_snapshot_sha256() => {}
            _ => {
                return Err(TaskError::InvalidRecord(
                    "Reason result publication provenance disagrees with its Task".into(),
                ));
            }
        }
        values.expected_result = Some(native_json(result.clone()));
        values.expected_metadata = Some(MetadataRecord(metadata));
        values.finding = Some(FindingRecord(output.finding));

        TaskContribution::settle(self.table.clone(), identity, values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_lookup_adapters_reject_unknown_known_fields() {
        let output: crate::contract::AnalyzeRecordingOutput =
            serde_json::from_str(include_str!("../testdata/analysis-output-v1.json")).unwrap();
        let good = native_json(serde_json::to_value(&output.finding).unwrap());
        assert!(FindingRecord::from_value(good).is_ok());
        let mut finding = serde_json::to_value(output.finding).unwrap();
        finding["answer"]["unknown"] = true.into();
        assert!(FindingRecord::from_value(native_json(finding)).is_err());
        assert!(
            MetadataRecord::from_value(native_json(output.results_artifact.metadata.clone()))
                .is_ok()
        );
        let mut metadata = output.results_artifact.metadata;
        metadata["provenance"]["unknown"] = true.into();
        assert!(MetadataRecord::from_value(native_json(metadata)).is_err());
    }
}
