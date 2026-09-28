//! Task status and typed resource invalidations share one authorized Task watch.
use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
};

use futures::{Stream, StreamExt};
use rmcp::{
    ErrorData as McpError,
    model::{DetailedTask, SubscriptionFilter},
    service::SubscriptionContext,
};
use veoveo_types::{ResourceUri, TaskId, TaskResourceAddress};

use crate::{DurableTaskService, types::parse_task_id};

pub struct TaskResourceUpdate {
    pub task: Option<DetailedTask>,
    pub resources: Vec<ResourceUri>,
}

pub type TaskResourceUpdateStream =
    Pin<Box<dyn Stream<Item = Result<TaskResourceUpdate, McpError>> + Send + 'static>>;

/// A checked request filter. Domain implementations own the address-to-Task relation.
/// Construction validates structure; the service authorizes the combined Task set.
pub struct TaskResourceSubscriptions {
    tasks: BTreeSet<TaskId>,
    resources: BTreeMap<TaskId, BTreeSet<ResourceUri>>,
}

impl TaskResourceSubscriptions {
    pub fn from_filter<A: TaskResourceAddress>(
        filter: &SubscriptionFilter,
    ) -> Result<Self, McpError> {
        if filter.resources_list_changed == Some(true)
            || filter.tools_list_changed == Some(true)
            || filter.prompts_list_changed == Some(true)
        {
            return Err(invalid("Task resources do not supply discovery changes"));
        }
        let requested_tasks = filter.task_ids.as_deref().unwrap_or_default();
        let requested_resources = filter.resource_subscriptions.as_deref().unwrap_or_default();
        if requested_tasks.len() > 256 || requested_resources.len() > 256 {
            return Err(invalid(
                "at most 256 Task IDs and 256 resources per subscription",
            ));
        }
        // Official Task admission ignores unknown handles; resource addresses must parse.
        let tasks = requested_tasks
            .iter()
            .filter_map(|id| parse_task_id(id).ok())
            .collect();
        let mut resources: BTreeMap<TaskId, BTreeSet<ResourceUri>> = BTreeMap::new();
        for uri in requested_resources {
            let reference =
                ResourceUri::new(uri).map_err(|_| invalid("invalid resource subscription"))?;
            let address =
                A::parse(&reference).map_err(|_| invalid("invalid resource subscription"))?;
            if address
                .to_uri()
                .map_err(|_| invalid("invalid resource subscription"))?
                != reference
            {
                return Err(invalid(
                    "resource subscription must use its canonical address",
                ));
            }
            let id = crate::types::validate_task_id(address.task_id())
                .map_err(|_| invalid("invalid resource Task identity"))?;
            resources.entry(id).or_default().insert(reference);
        }
        let selection = Self { tasks, resources };
        if selection.task_ids().len() > 256 {
            return Err(invalid("at most 256 distinct Tasks per subscription"));
        }
        Ok(selection)
    }

    /// Domain adapters may apply additional resource admission before subscribing.
    pub fn resource_task_ids(&self) -> impl Iterator<Item = TaskId> + '_ {
        self.resources.keys().copied()
    }

    fn task_ids(&self) -> BTreeSet<TaskId> {
        self.tasks
            .iter()
            .chain(self.resources.keys())
            .copied()
            .collect()
    }

    pub async fn subscribe<S: DurableTaskService>(
        self,
        service: &S,
        caller: &S::Caller,
    ) -> Result<TaskResourceUpdateStream, McpError> {
        let selected = self.task_ids();
        if selected.is_empty() {
            return Ok(Box::pin(futures::stream::pending()));
        }
        let subscription = service
            .subscribe_tasks(caller, selected.iter().map(ToString::to_string).collect())
            .await?;
        let admitted = subscription
            .accepted_task_ids
            .iter()
            .map(|id| parse_task_id(id).map_err(|_| invalid("invalid subscribed Task identity")))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if !admitted.is_subset(&selected) {
            return Err(McpError::internal_error(
                "Task subscription returned an unrequested identity",
                None,
            ));
        }
        if self.resources.keys().any(|id| !admitted.contains(id)) {
            return Err(McpError::resource_not_found(
                "resource Task is unavailable",
                None,
            ));
        }
        let mut updates = subscription.updates;
        Ok(Box::pin(async_stream::try_stream! {
            while let Some(update) = updates.next().await {
                let update = update?;
                let id = parse_task_id(&update.task.task_id)
                    .map_err(|_| McpError::internal_error("invalid Task update identity", None))?;
                if !admitted.contains(&id) {
                    Err(McpError::internal_error("Task update returned an unrequested identity", None))?;
                }
                let resources = self.resources.get(&id).map(|uris| uris.iter().cloned().collect()).unwrap_or_default();
                yield TaskResourceUpdate {
                    task: self.tasks.contains(&id).then_some(update),
                    resources,
                };
            }
        }))
    }

    pub async fn listen<S: DurableTaskService>(
        self,
        service: &S,
        context: SubscriptionContext,
    ) -> Result<(), McpError> {
        let caller = service.authenticate(context.request_context())?;
        let mut updates = self.subscribe(service, &caller).await?;
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                update = updates.next() => {
                    let Some(update) = update else { return Ok(()); };
                    let update = update?;
                    if let Some(task) = update.task {
                        context.sink().notify_task_status(task).await
                            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                    }
                    for uri in update.resources {
                        context.sink().notify_resource_updated(uri.to_string()).await
                            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                    }
                }
            }
        }
    }
}

fn invalid(message: &'static str) -> McpError {
    McpError::invalid_params(message, None)
}
