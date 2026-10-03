//! Durable tasks for servers hosted through `veoveo_mcp_contract::hosting`.
//!
//! [`DurableTasks`] adapts a server's [`DurableTaskService`] to the host's
//! `TaskSupport`. The host then starts task-augmented tool calls and serves
//! `tasks/get`, `tasks/update` and `tasks/cancel` under the authenticated caller.
//! A [`DurableListener`] decides what `subscriptions/listen` observes:
//!
//! - [`DurableTasks::tasks_only`]: task updates only;
//! - [`DurableTasks::with_resources`]: task updates and the typed resource
//!   changes of a `ResourceSubscriptions` source;
//! - [`DurableTasks::with_listener`]: a server-specific listener, for servers
//!   whose subscriptions combine tasks with live or domain event streams.

use std::future::Future;

use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CancelTaskParams, CreateTaskResult, GetTaskParams, GetTaskResult,
        SubscriptionFilter, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
};
use veoveo_mcp_contract::{
    accepted_subscription_filter, accepted_task_subscription_filter,
    hosting::{ResourceSubscriptions, TaskSupport, admit_resource_subscriptions},
};

use crate::{DurableTaskService, listen_durable_subscriptions, start_durable_tool_task};

/// What a server's `subscriptions/listen` observes.
pub trait DurableListener<S: DurableTaskService>: Send + Sync + 'static {
    /// The subset of a requested filter the server accepts.
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter>;

    /// Delivers updates for the accepted filter until the request ends.
    fn listen(
        &self,
        service: &S,
        context: SubscriptionContext,
    ) -> impl Future<Output = Result<(), McpError>> + Send;
}

/// Task updates only; resource observations are not accepted.
pub struct TasksOnly;

impl<S: DurableTaskService> DurableListener<S> for TasksOnly {
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        accepted_task_subscription_filter(requested)
    }

    async fn listen(&self, service: &S, context: SubscriptionContext) -> Result<(), McpError> {
        listen_durable_subscriptions(service, context, None, None).await
    }
}

/// Task updates and the changes a [`ResourceSubscriptions`] source publishes.
/// The host parses each requested resource URI into the source's typed address
/// and the source authorizes them before delivery starts.
pub struct WithResources<R>(R);

impl<S: DurableTaskService, R: ResourceSubscriptions> DurableListener<S> for WithResources<R> {
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        accepted_subscription_filter(requested)
    }

    async fn listen(&self, service: &S, context: SubscriptionContext) -> Result<(), McpError> {
        admit_resource_subscriptions(&self.0, &context).await?;
        listen_durable_subscriptions(
            service,
            context,
            Some(self.0.hub()),
            self.0.resource_lists(),
        )
        .await
    }
}

/// Durable task support for a hosted server.
pub struct DurableTasks<S, L = TasksOnly> {
    service: S,
    listener: L,
}

impl<S: DurableTaskService> DurableTasks<S> {
    /// Admits task subscriptions and rejects resource observations, for servers
    /// that publish no resource changes.
    pub fn tasks_only(service: S) -> Self {
        Self {
            service,
            listener: TasksOnly,
        }
    }

    /// Observes tasks and the typed resource changes `resources` publishes.
    pub fn with_resources<R: ResourceSubscriptions>(
        service: S,
        resources: R,
    ) -> DurableTasks<S, WithResources<R>> {
        DurableTasks {
            service,
            listener: WithResources(resources),
        }
    }

    /// Observes what a server-specific `listener` delivers.
    pub fn with_listener<L: DurableListener<S>>(service: S, listener: L) -> DurableTasks<S, L> {
        DurableTasks { service, listener }
    }
}

impl<S: DurableTaskService, L> DurableTasks<S, L> {
    pub fn service(&self) -> &S {
        &self.service
    }
}

impl<S: DurableTaskService, L: DurableListener<S>> TaskSupport for DurableTasks<S, L> {
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
        self.listener.accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        self.listener.listen(&self.service, context).await
    }
}
