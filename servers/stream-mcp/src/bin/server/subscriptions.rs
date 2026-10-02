//! Compose shared run updates with the process that owns each live session.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use futures::StreamExt;
use rmcp::{ErrorData as McpError, model::SubscriptionFilter, service::SubscriptionContext};
use tokio::sync::broadcast;
use veoveo_mcp_contract::{ResourceUpdate, receive_resource_update};
use veoveo_stream_mcp::contract::{RunId, RunResource, SessionId, StreamResource};
use veoveo_task_runtime::{
    DurableTaskService, TaskOwner, TaskResourceSubscriptions, TaskResourceUpdate,
    TaskResourceUpdateStream, TaskRuntime,
};
use veoveo_types::{ResourceAddress, ResourceUri};

use super::{internal, live::LiveSessionManager, resources::run_snapshot};

struct Selection {
    runs: TaskResourceSubscriptions,
    sessions: BTreeMap<SessionId, BTreeSet<ResourceUri>>,
}

impl Selection {
    fn new(filter: &SubscriptionFilter) -> Result<Self, McpError> {
        let resources = filter.resource_subscriptions.as_deref().unwrap_or_default();
        if resources.len() > 256 {
            return Err(McpError::invalid_params(
                "at most 256 resources per subscription",
                None,
            ));
        }
        let mut runs = filter.clone();
        let mut run_uris = Vec::new();
        let mut sessions: BTreeMap<SessionId, BTreeSet<ResourceUri>> = BTreeMap::new();
        for uri in resources {
            let resource = StreamResource::parse(uri).map_err(|_| invalid_resource())?;
            if let Some(id) = resource.subscription_session() {
                sessions
                    .entry(id)
                    .or_default()
                    .insert(resource.to_uri().map_err(|_| invalid_resource())?);
            } else if resource.subscription_run().is_some() {
                run_uris.push(uri.clone());
            } else {
                return Err(invalid_resource());
            }
        }
        runs.resource_subscriptions = Some(run_uris).filter(|uris| !uris.is_empty());
        Ok(Self {
            runs: TaskResourceSubscriptions::from_filter::<RunResource>(&runs)?,
            sessions,
        })
    }

    async fn subscribe<S: DurableTaskService>(
        self,
        service: &S,
        caller: &S::Caller,
        runtime: &TaskRuntime,
        live: Arc<LiveSessionManager>,
        owner: TaskOwner,
    ) -> Result<Updates, McpError> {
        // Attach before admission so changes during authorization remain observable.
        let live_updates = (!self.sessions.is_empty()).then(|| live.listen_updates());
        for id in self.sessions.keys() {
            authorize_session(&live, &owner, *id).await?;
        }
        for id in self.runs.resource_task_ids() {
            // Task authorization alone cannot establish the Stream operation kind.
            run_snapshot(
                runtime,
                &owner,
                RunId::try_from(id).map_err(|_| invalid_resource())?,
            )
            .await?;
        }
        let tasks = self.runs.subscribe(service, caller).await?;
        Ok(Updates {
            tasks,
            live_updates,
            initial_live: !self.sessions.is_empty(),
            sessions: self.sessions,
            live,
            owner,
        })
    }
}

struct Updates {
    tasks: TaskResourceUpdateStream,
    live_updates: Option<broadcast::Receiver<ResourceUpdate>>,
    initial_live: bool,
    sessions: BTreeMap<SessionId, BTreeSet<ResourceUri>>,
    live: Arc<LiveSessionManager>,
    owner: TaskOwner,
}

impl Updates {
    async fn next(&mut self) -> Option<Result<TaskResourceUpdate, McpError>> {
        loop {
            let update = if std::mem::take(&mut self.initial_live) {
                // Reconnecting clients read current live state even without a new frame.
                ResourceUpdate::Reconcile
            } else {
                tokio::select! {
                    task = self.tasks.next() => return task,
                    update = receive_resource_update(&mut self.live_updates) => update,
                }
            };
            let mut resources = Vec::new();
            for (id, addresses) in &self.sessions {
                let selected = addresses
                    .iter()
                    .filter(|uri| match &update {
                        ResourceUpdate::Reconcile => true,
                        ResourceUpdate::Uri(changed) => uri.as_str() == changed,
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if !selected.is_empty() {
                    if let Err(error) = authorize_session(&self.live, &self.owner, *id).await {
                        return Some(Err(error));
                    }
                    resources.extend(selected);
                }
            }
            if !resources.is_empty() {
                return Some(Ok(TaskResourceUpdate {
                    task: None,
                    resources,
                }));
            }
        }
    }
}

pub(super) async fn listen<S: DurableTaskService>(
    service: &S,
    caller: &S::Caller,
    runtime: &TaskRuntime,
    live: Arc<LiveSessionManager>,
    owner: TaskOwner,
    context: SubscriptionContext,
) -> Result<(), McpError> {
    let mut updates = Selection::new(context.accepted())?
        .subscribe(service, caller, runtime, live, owner)
        .await?;
    loop {
        tokio::select! {
            () = context.cancelled() => return Ok(()),
            update = updates.next() => {
                let Some(update) = update else { return Ok(()); };
                let update = update?;
                if let Some(task) = update.task {
                    context.sink().notify_task_status(task).await.map_err(internal)?;
                }
                for uri in update.resources {
                    context.sink().notify_resource_updated(uri.to_string()).await.map_err(internal)?;
                }
            }
        }
    }
}

async fn authorize_session(
    live: &LiveSessionManager,
    owner: &TaskOwner,
    id: SessionId,
) -> Result<(), McpError> {
    if live.readable_by(id, owner).await {
        Ok(())
    } else {
        Err(McpError::resource_not_found(
            "Stream session not found",
            None,
        ))
    }
}

fn invalid_resource() -> McpError {
    McpError::invalid_params("invalid Stream resource subscription", None)
}

#[cfg(test)]
#[path = "subscriptions_tests.rs"]
mod tests;
