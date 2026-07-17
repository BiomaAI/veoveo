use std::{collections::BTreeSet, pin::Pin, sync::Arc};

use axum::http::StatusCode;
use futures::{Stream, StreamExt, stream};
use rmcp::model::{ErrorCode, ErrorData as McpError};
use veoveo_mcp_contract::{GatewayAction, PolicyReasonCode, PolicyTarget};
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, AdapterError, CancelTaskParams, CreateTaskResult, DetailedTask,
    GetTaskParams, GetTaskResult, ProtocolTaskId, TaskExtensionHandler, TaskSubscription,
    ToolCallParams, UpdateTaskParams,
};

use crate::{AuthenticatedSubject, mcp_support::parse_gateway_tool};

use super::{GatewayMcp, tools::project_detailed_task_resource_uris};

type GatewayTaskStream =
    Pin<Box<dyn Stream<Item = Result<DetailedTask, AdapterError>> + Send + 'static>>;

#[derive(Clone)]
pub struct GatewayTaskExtension {
    gateway: Arc<GatewayMcp>,
}

impl GatewayTaskExtension {
    pub fn new(gateway: GatewayMcp) -> Self {
        Self {
            gateway: Arc::new(gateway),
        }
    }

    fn task_manifest(
        &self,
        server: &veoveo_mcp_contract::ServerSlug,
    ) -> Result<veoveo_mcp_contract::ServerManifest, AdapterError> {
        self.gateway
            .catalog
            .current()
            .profile_servers(&self.gateway.profile_id)
            .into_iter()
            .find_map(|(exposure, manifest)| {
                (manifest.slug == *server
                    && exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled
                    && manifest.capabilities.tasks)
                    .then(|| manifest.clone())
            })
            .ok_or_else(|| AdapterError::invalid_params("unknown task-capable server"))
    }
}

impl TaskExtensionHandler for GatewayTaskExtension {
    type Caller = AuthenticatedSubject;

    fn authenticate(
        &self,
        extensions: &axum::http::Extensions,
    ) -> Result<Self::Caller, AdapterError> {
        extensions
            .get::<AuthenticatedSubject>()
            .cloned()
            .ok_or_else(|| AdapterError::unauthorized("authenticated subject missing"))
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        mut request: ToolCallParams,
    ) -> Result<Option<CreateTaskResult>, AdapterError> {
        let catalog = self.gateway.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name).map_err(adapter_error)?;
        drop(catalog);
        self.task_manifest(&projection.server)?;
        if !self
            .gateway
            .client_allows_compatibility_helper(caller, &projection.server, &projection.tool)
            .map_err(adapter_error)?
        {
            self.gateway
                .record_policy_denial(
                    caller,
                    GatewayAction::ToolsCall,
                    PolicyTarget::Tool {
                        server: projection.server,
                        tool: projection.tool,
                    },
                    PolicyReasonCode::UnknownTool,
                )
                .await
                .map_err(adapter_error)?;
            return Err(AdapterError::invalid_params("unknown tool"));
        }
        let subject = self
            .gateway
            .authorize_tool_for_subject(
                caller,
                GatewayAction::ToolsCall,
                projection.server.clone(),
                projection.tool.clone(),
            )
            .await
            .map_err(adapter_error)?;
        request.name = projection.tool.to_string();
        let client = self
            .gateway
            .final_task_client(&projection.server, &subject)
            .await
            .map_err(adapter_error)?;
        client
            .start_tool(request)
            .await
            .map(Some)
            .map_err(adapter_error)
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: GetTaskParams,
    ) -> Result<GetTaskResult, AdapterError> {
        let route = self
            .gateway
            .authorize_canonical_task_for_subject(
                caller,
                GatewayAction::TasksGet,
                &request.task_id.to_string(),
            )
            .await
            .map_err(adapter_error)?;
        let manifest = self.task_manifest(&route.server)?;
        let client = self
            .gateway
            .final_task_client(&route.server, &route.subject)
            .await
            .map_err(adapter_error)?;
        let mut task = client.get(route.task_id).await.map_err(adapter_error)?;
        project_detailed_task_resource_uris(&manifest, &mut task).map_err(adapter_error)?;
        Ok(GetTaskResult::new(task))
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        let route = self
            .gateway
            .authorize_canonical_task_for_subject(
                caller,
                GatewayAction::TasksUpdate,
                &request.task_id.to_string(),
            )
            .await
            .map_err(adapter_error)?;
        let client = self
            .gateway
            .final_task_client(&route.server, &route.subject)
            .await
            .map_err(adapter_error)?;
        client
            .update(route.task_id, request.input_responses)
            .await
            .map_err(adapter_error)
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        request: CancelTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        let route = self
            .gateway
            .authorize_canonical_task_for_subject(
                caller,
                GatewayAction::TasksCancel,
                &request.task_id.to_string(),
            )
            .await
            .map_err(adapter_error)?;
        let client = self
            .gateway
            .final_task_client(&route.server, &route.subject)
            .await
            .map_err(adapter_error)?;
        client.cancel(route.task_id).await.map_err(adapter_error)
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<ProtocolTaskId>,
    ) -> Result<TaskSubscription, AdapterError> {
        let mut accepted = Vec::new();
        let mut seen = BTreeSet::new();
        let mut updates: Vec<GatewayTaskStream> = Vec::new();
        for task_id in task_ids {
            if !seen.insert(task_id) {
                continue;
            }
            let route = match self
                .gateway
                .authorize_canonical_task_for_subject(
                    caller,
                    GatewayAction::TasksSubscribe,
                    &task_id.to_string(),
                )
                .await
            {
                Ok(route) => route,
                Err(error) if error.code == ErrorCode::INTERNAL_ERROR => {
                    return Err(adapter_error(error));
                }
                Err(_) => continue,
            };
            let manifest = self.task_manifest(&route.server)?;
            let client = self
                .gateway
                .final_task_client(&route.server, &route.subject)
                .await
                .map_err(adapter_error)?;
            let stream = client
                .subscribe(vec![route.task_id])
                .await
                .map_err(adapter_error)?
                .map(move |update| {
                    let mut update = update.map_err(adapter_error)?;
                    project_detailed_task_resource_uris(&manifest, &mut update)
                        .map_err(adapter_error)?;
                    Ok(update)
                });
            accepted.push(route.task_id);
            updates.push(Box::pin(stream));
        }
        Ok(TaskSubscription {
            accepted_task_ids: accepted,
            updates: Box::pin(stream::select_all(updates)),
        })
    }
}

fn adapter_error(error: McpError) -> AdapterError {
    let http_status = if error.code == ErrorCode::INTERNAL_ERROR {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    };
    AdapterError {
        code: error.code.0,
        message: error.message.into_owned(),
        data: error.data,
        http_status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_gateway_json_rpc_error_details() {
        let projected = adapter_error(McpError::invalid_params(
            "unknown canonical task",
            Some(serde_json::json!({"field": "taskId"})),
        ));
        assert_eq!(projected.code, ErrorCode::INVALID_PARAMS.0);
        assert_eq!(projected.http_status, StatusCode::BAD_REQUEST);
        assert_eq!(projected.data, Some(serde_json::json!({"field": "taskId"})));
    }
}
