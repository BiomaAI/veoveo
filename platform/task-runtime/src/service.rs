//! Protocol-neutral service boundary for durable official MCP Tasks handlers.

use std::{collections::BTreeSet, pin::Pin};

use futures::{Stream, StreamExt};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CreateTaskResult, DetailedTask, GetTaskParams, GetTaskResult,
        RequestMetaObject, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext, SubscriptionSendError},
};
use veoveo_mcp_contract::{ResourceListObservers, SubscriptionHub};

use crate::{TaskError, TaskOwner, TaskRetentionPin, TaskRuntime, TaskSnapshot, project_snapshot};

/// Repository-owned metadata used only to retain internal evidence longer than wire TTL.
pub const TASK_RETENTION_PIN_META_KEY: &str = "ai.bioma.veoveo/taskRetentionPin";

pub type DurableTaskUpdateStream =
    Pin<Box<dyn Stream<Item = Result<DetailedTask, McpError>> + Send + 'static>>;

/// Authorized task updates accepted for one `subscriptions/listen` request.
pub struct DurableTaskSubscription {
    pub accepted_task_ids: Vec<String>,
    pub updates: DurableTaskUpdateStream,
}

/// Domain adapter used by a server's RMCP `ServerHandler` implementation.
///
/// This trait owns no wire parsing, transport, or protocol models. RMCP dispatches the
/// official methods and the hosted server delegates durable domain work here.
pub trait DurableTaskService: Send + Sync + 'static {
    type Caller: Clone + Send + Sync + 'static;

    fn authenticate(&self, context: &RequestContext<RoleServer>) -> Result<Self::Caller, McpError>;

    fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: CallToolRequestParams,
    ) -> impl Future<Output = Result<Option<CreateTaskResult>, McpError>> + Send;

    fn get_task(
        &self,
        caller: &Self::Caller,
        request: GetTaskParams,
    ) -> impl Future<Output = Result<GetTaskResult, McpError>> + Send;

    fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> impl Future<Output = Result<(), McpError>> + Send;

    fn cancel_task(
        &self,
        caller: &Self::Caller,
        task_id: String,
    ) -> impl Future<Output = Result<(), McpError>> + Send;

    fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> impl Future<Output = Result<DurableTaskSubscription, McpError>> + Send;
}

/// Extracts the optional internal retention pin from ordinary request metadata.
pub fn retention_pins(
    meta: Option<&RequestMetaObject>,
) -> Result<BTreeSet<TaskRetentionPin>, McpError> {
    meta.and_then(|meta| meta.get(TASK_RETENTION_PIN_META_KEY))
        .cloned()
        .map(serde_json::from_value::<TaskRetentionPin>)
        .transpose()
        .map_err(|error| McpError::invalid_params(error.to_string(), None))
        .map(Option::into_iter)
        .map(Iterator::collect)
}

/// Converts official heterogeneous input responses to the durable object form.
pub fn durable_input_responses(
    request: UpdateTaskParams,
) -> Result<
    std::collections::BTreeMap<String, std::collections::BTreeMap<String, serde_json::Value>>,
    McpError,
> {
    request
        .input_responses
        .into_iter()
        .map(|(key, value)| {
            value
                .as_object()
                .cloned()
                .map(|value| (key, value.into_iter().collect()))
                .ok_or_else(|| {
                    McpError::invalid_params("task input responses must be objects", None)
                })
        })
        .collect()
}

/// Loads a first-party durable task and enforces its retained owner on every use.
pub async fn authorized_snapshot(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    task_id: &str,
) -> Result<TaskSnapshot, McpError> {
    task_id
        .parse::<crate::TaskId>()
        .map_err(|_| McpError::invalid_params("unknown task id", None))?;
    let snapshot = runtime
        .get(task_id)
        .await
        .map_err(task_error)?
        .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
    if snapshot.owner.allows(
        &owner.principal_key,
        &owner.profile,
        owner.tenant_key.as_deref(),
        &owner.data_labels,
    ) {
        Ok(snapshot)
    } else {
        Err(McpError::invalid_params("unknown task id", None))
    }
}

pub async fn get_durable_task(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    request: GetTaskParams,
) -> Result<GetTaskResult, McpError> {
    let snapshot = authorized_snapshot(runtime, owner, &request.task_id).await?;
    project_snapshot(runtime, snapshot)
        .await
        .map(GetTaskResult::new)
        .map_err(task_error)
}

pub async fn update_durable_task(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    request: UpdateTaskParams,
) -> Result<(), McpError> {
    authorized_snapshot(runtime, owner, &request.task_id).await?;
    let task_id = request.task_id.clone();
    let responses = durable_input_responses(request)?;
    runtime
        .submit_input_responses(&task_id, responses)
        .await
        .map_err(task_error)?;
    Ok(())
}

pub async fn cancel_durable_task(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    task_id: String,
) -> Result<(), McpError> {
    authorized_snapshot(runtime, owner, &task_id).await?;
    runtime.cancel(&task_id).await.map_err(task_error)?;
    Ok(())
}

pub async fn subscribe_durable_tasks(
    runtime: &TaskRuntime,
    owner: TaskOwner,
    task_ids: Vec<String>,
) -> Result<DurableTaskSubscription, McpError> {
    let updates = runtime.live_updates().await.map_err(task_error)?;
    let mut accepted = Vec::new();
    for task_id in task_ids {
        if authorized_snapshot(runtime, &owner, &task_id).await.is_ok() {
            accepted.push(task_id);
        }
    }
    let accepted_set: BTreeSet<_> = accepted.iter().cloned().collect();
    let runtime = runtime.clone();
    let stream = updates.filter_map(move |update| {
        let accepted = accepted_set.clone();
        let runtime = runtime.clone();
        let owner = owner.clone();
        async move {
            let snapshot = match update {
                Ok(update) => update.snapshot,
                Err(error) => return Some(Err(task_error(error))),
            };
            if !accepted.contains(&snapshot.task_id.to_string())
                || !snapshot.owner.allows(
                    &owner.principal_key,
                    &owner.profile,
                    owner.tenant_key.as_deref(),
                    &owner.data_labels,
                )
            {
                return None;
            }
            Some(
                project_snapshot(&runtime, snapshot)
                    .await
                    .map_err(task_error),
            )
        }
    });
    Ok(DurableTaskSubscription {
        accepted_task_ids: accepted,
        updates: Box::pin(stream),
    })
}

fn subscription_send_error(error: SubscriptionSendError) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

/// Runs one official Tasks listener, optionally multiplexed with the hosted
/// server's authorized resource and resource-list update broadcasters.
///
/// Every accepted resource URI must be authorized before calling this helper.
pub async fn listen_durable_subscriptions<S: DurableTaskService>(
    service: &S,
    context: SubscriptionContext,
    resources: Option<&SubscriptionHub>,
    resource_lists: Option<&ResourceListObservers>,
) -> Result<(), McpError> {
    let accepted = context.accepted().clone();
    let task_ids = accepted.task_ids.clone().unwrap_or_default();
    let mut tasks: DurableTaskUpdateStream = if task_ids.is_empty() {
        Box::pin(futures::stream::pending())
    } else {
        let caller = service.authenticate(context.request_context())?;
        service.subscribe_tasks(&caller, task_ids).await?.updates
    };
    let mut resource_updates = resources.map(SubscriptionHub::listen);
    let mut hub_list_changes = resources.map(SubscriptionHub::listen_resource_list_changes);
    let mut extra_list_changes = resource_lists.map(ResourceListObservers::listen);

    loop {
        tokio::select! {
            () = context.cancelled() => return Ok(()),
            update = tasks.next() => {
                let Some(update) = update else { return Ok(()); };
                context.sink().notify_task_status(update?).await.map_err(subscription_send_error)?;
            }
            uri = veoveo_mcp_contract::receive_resource_update(&mut resource_updates) => {
                if accepted.resource_subscriptions.as_ref().is_some_and(|uris| uris.contains(&uri)) {
                    context.sink().notify_resource_updated(uri).await.map_err(subscription_send_error)?;
                }
            }
            () = veoveo_mcp_contract::receive_resource_list_change(&mut hub_list_changes), if accepted.resources_list_changed == Some(true) => {
                context.sink().notify_resource_list_changed().await.map_err(subscription_send_error)?;
            }
            () = veoveo_mcp_contract::receive_resource_list_change(&mut extra_list_changes), if accepted.resources_list_changed == Some(true) => {
                context.sink().notify_resource_list_changed().await.map_err(subscription_send_error)?;
            }
        }
    }
}

fn task_error(error: TaskError) -> McpError {
    match error {
        TaskError::NotFound(_) | TaskError::WrongServer(_) => {
            McpError::invalid_params("unknown task id", None)
        }
        other => McpError::internal_error(other.to_string(), None),
    }
}

/// Adds official Tasks dispatch to a hosted server while preserving its ordinary tool router.
#[macro_export]
macro_rules! durable_task_handlers {
    ($task_service:ident, $tool_router:ident) => {
        async fn call_tool(
            &self,
            request: rmcp::model::CallToolRequestParams,
            context: rmcp::service::RequestContext<rmcp::RoleServer>,
        ) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
            if context
                .meta
                .client_capabilities()
                .is_some_and(|capabilities| capabilities.supports_tasks())
            {
                let caller =
                    $crate::DurableTaskService::authenticate(&self.$task_service, &context)?;
                if let Some(created) = $crate::DurableTaskService::start_tool_task(
                    &self.$task_service,
                    &caller,
                    request.clone(),
                )
                .await?
                {
                    return Ok(created.into());
                }
            }
            let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
            self.$tool_router.call(call).await
        }

        async fn get_task(
            &self,
            request: rmcp::model::GetTaskParams,
            context: rmcp::service::RequestContext<rmcp::RoleServer>,
        ) -> Result<rmcp::model::GetTaskResult, rmcp::ErrorData> {
            let caller = $crate::DurableTaskService::authenticate(&self.$task_service, &context)?;
            $crate::DurableTaskService::get_task(&self.$task_service, &caller, request).await
        }

        async fn update_task(
            &self,
            request: rmcp::model::UpdateTaskParams,
            context: rmcp::service::RequestContext<rmcp::RoleServer>,
        ) -> Result<(), rmcp::ErrorData> {
            let caller = $crate::DurableTaskService::authenticate(&self.$task_service, &context)?;
            $crate::DurableTaskService::update_task(&self.$task_service, &caller, request).await
        }

        async fn cancel_task(
            &self,
            request: rmcp::model::CancelTaskParams,
            context: rmcp::service::RequestContext<rmcp::RoleServer>,
        ) -> Result<(), rmcp::ErrorData> {
            let caller = $crate::DurableTaskService::authenticate(&self.$task_service, &context)?;
            $crate::DurableTaskService::cancel_task(&self.$task_service, &caller, request.task_id)
                .await
        }
    };
}
