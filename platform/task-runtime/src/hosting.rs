//! Durable tasks for servers hosted through `veoveo_mcp_contract::hosting`.
//!
//! [`DurableTasks`] adapts a server's [`DurableTaskService`] to the host's
//! `TaskSupport`. The host then starts task-augmented tool calls, serves
//! `tasks/get`, `tasks/update` and `tasks/cancel` under the authenticated caller,
//! and delivers task updates through `subscriptions/listen`.

use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CancelTaskParams, CreateTaskResult, GetTaskParams, GetTaskResult,
        SubscriptionFilter, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
};
use veoveo_mcp_contract::{accepted_task_subscription_filter, hosting::TaskSupport};

use crate::{DurableTaskService, listen_durable_subscriptions, start_durable_tool_task};

/// Task support for a server whose subscriptions observe only its tasks.
#[derive(Clone)]
pub struct DurableTasks<S> {
    service: S,
}

impl<S: DurableTaskService> DurableTasks<S> {
    /// Admits task subscriptions and rejects resource observations, for servers
    /// that publish no resource changes.
    pub fn tasks_only(service: S) -> Self {
        Self { service }
    }

    pub fn service(&self) -> &S {
        &self.service
    }
}

impl<S: DurableTaskService> TaskSupport for DurableTasks<S> {
    async fn start_task(
        &self,
        request: &mut CallToolRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<Option<CreateTaskResult>, McpError> {
        start_durable_tool_task(&self.service, request, context).await
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let caller = self.service.authenticate(&context)?;
        self.service.get_task(&caller, request).await
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller = self.service.authenticate(&context)?;
        self.service.update_task(&caller, request).await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller = self.service.authenticate(&context)?;
        self.service.cancel_task(&caller, request.task_id).await
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        accepted_task_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        listen_durable_subscriptions(&self.service, context, None, None).await
    }
}
