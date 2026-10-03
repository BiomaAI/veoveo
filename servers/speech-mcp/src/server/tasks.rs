use crate::application::{SpeechService, owner};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CreateTaskResult, GetTaskParams, GetTaskResult, UpdateTaskParams,
    },
    service::RequestContext,
};
use std::sync::Arc;
use veoveo_mcp_contract::PlaneCaller;
use veoveo_speech_contract::{TranscribeRequest, TranscriptionId};
use veoveo_task_runtime::{
    DurableTaskService, DurableTaskSubscription, cancel_durable_task, get_durable_task,
    retention_pins, subscribe_durable_tasks, task_seed, update_durable_task,
};

#[derive(Clone)]
pub(super) struct SpeechTasks(pub Arc<SpeechService>);

pub(super) fn caller(context: &RequestContext<RoleServer>) -> Result<PlaneCaller, McpError> {
    veoveo_mcp_contract::hosting::plane_caller(context)
}

impl DurableTaskService for SpeechTasks {
    type Caller = PlaneCaller;
    fn authenticate(&self, context: &RequestContext<RoleServer>) -> Result<Self::Caller, McpError> {
        caller(context)
    }
    async fn start_tool_task(
        &self,
        caller: &PlaneCaller,
        request: CallToolRequestParams,
    ) -> Result<Option<CreateTaskResult>, McpError> {
        if request.name != "transcribe" {
            return Ok(None);
        }
        let input: TranscribeRequest = serde_json::from_value(serde_json::Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|_| {
            McpError::invalid_params("a canonical source Artifact URI is required", None)
        })?;
        let task = self
            .0
            .transcribe(caller, input, retention_pins(request.meta.as_ref())?)
            .await
            .map_err(|_| {
                McpError::invalid_params(
                    "The recording cannot be transcribed with current access or source limits.",
                    None,
                )
            })?;
        Ok(Some(CreateTaskResult::new(task_seed(&task))))
    }
    async fn get_task(
        &self,
        caller: &PlaneCaller,
        request: GetTaskParams,
    ) -> Result<GetTaskResult, McpError> {
        self.0
            .authorize(caller, transcription_id(&request.task_id)?, true)
            .await?;
        get_durable_task(&self.0.tasks.for_owner(&owner(&caller.identity)), request).await
    }
    async fn update_task(
        &self,
        caller: &PlaneCaller,
        request: UpdateTaskParams,
    ) -> Result<(), McpError> {
        self.0
            .authorize(caller, transcription_id(&request.task_id)?, false)
            .await?;
        update_durable_task(&self.0.tasks.for_owner(&owner(&caller.identity)), request).await
    }
    async fn cancel_task(&self, caller: &PlaneCaller, task_id: String) -> Result<(), McpError> {
        self.0
            .authorize(caller, transcription_id(&task_id)?, false)
            .await?;
        cancel_durable_task(&self.0.tasks.for_owner(&owner(&caller.identity)), task_id).await
    }
    async fn subscribe_tasks(
        &self,
        caller: &PlaneCaller,
        task_ids: Vec<String>,
    ) -> Result<DurableTaskSubscription, McpError> {
        for id in &task_ids {
            self.0
                .authorize(caller, transcription_id(id)?, true)
                .await?;
        }
        subscribe_durable_tasks(&self.0.tasks.for_owner(&owner(&caller.identity)), task_ids).await
    }
}

pub(super) fn transcription_id(value: &str) -> Result<TranscriptionId, McpError> {
    TranscriptionId::parse(value)
        .map_err(|_| McpError::invalid_params("unknown transcription", None))
}
