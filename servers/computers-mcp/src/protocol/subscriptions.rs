use super::{
    ComputersMcp, auth,
    resources::{self, ResourceId},
};
use futures::StreamExt;
use rmcp::{
    ErrorData, RoleServer,
    service::{RequestContext, SubscriptionContext},
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use veoveo_computers::ComputerActor;
use veoveo_platform_store::{OutboxEventRecord, PlatformTable};
use veoveo_task_runtime::{DurableTaskUpdateStream, TaskOwner};

struct ListenerAuthority {
    actor: ComputerActor,
    deadline: Instant,
    tasks: BTreeMap<String, TaskOwner>,
}

impl ComputersMcp {
    async fn validate_listener(
        &self,
        request: &RequestContext<RoleServer>,
        uris: &[String],
        tasks: &[String],
    ) -> Result<ListenerAuthority, ErrorData> {
        let actor = auth::actor(request)?;
        let control = self
            .app
            .store
            .control_authority(&actor)
            .await
            .map_err(|_| auth::forbidden())?;
        for uri in uris {
            match resources::parse(uri) {
                Some(ResourceId::Collection(_)) => {
                    control.require_read(None).map_err(|_| auth::forbidden())?
                }
                Some(
                    ResourceId::Computer(id)
                    | ResourceId::Access(id)
                    | ResourceId::Automation(id)
                    | ResourceId::Maintenance(id),
                ) => {
                    control
                        .require_read(Some(id))
                        .map_err(|_| auth::forbidden())?;
                    self.app
                        .store
                        .get(actor.owner(), id)
                        .await
                        .map_err(|_| auth::forbidden())?;
                }
                Some(ResourceId::Grant(computer, grant)) => {
                    self.app
                        .automation_grant(&actor, computer, grant)
                        .await
                        .map_err(super::read_error)?;
                }
                _ => {
                    return Err(ErrorData::invalid_params(
                        "resource is not subscribable",
                        None,
                    ));
                }
            }
        }
        let mut deadline = control.valid_until();
        let mut owners = BTreeMap::new();
        for id in tasks {
            let access = self.task_access_for_actor(&actor, id, false).await?;
            deadline = deadline.min(access.deadline);
            owners.insert(id.clone(), access.owner);
        }
        Ok(ListenerAuthority {
            actor,
            deadline,
            tasks: owners,
        })
    }
    pub(super) async fn listen_updates(
        &self,
        context: SubscriptionContext,
    ) -> Result<(), ErrorData> {
        let _slot = self
            .app
            .subscription_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                ErrorData::internal_error("Computer subscription capacity is full", None)
            })?;
        let uris = context
            .accepted()
            .resource_subscriptions
            .clone()
            .unwrap_or_default();
        let task_ids = context.accepted().task_ids.clone().unwrap_or_default();
        if uris.len() + task_ids.len() > 64 {
            return Err(ErrorData::invalid_params(
                "at most 64 subscription targets are supported",
                None,
            ));
        }
        let authority = tokio::time::timeout(
            Duration::from_secs(5),
            self.validate_listener(context.request_context(), &uris, &task_ids),
        )
        .await
        .map_err(|_| auth::unavailable())??;
        let actor = &authority.actor;
        let pump = async {
            let mut tasks: DurableTaskUpdateStream = if task_ids.is_empty() {
                Box::pin(futures::stream::pending())
            } else {
                // One shared wake source serves independently authorized Task
                // owners. Computer ownership and execution actor need not coincide.
                let updates = self
                    .app
                    .tasks
                    .live_updates()
                    .await
                    .map_err(|_| auth::unavailable())?;
                let owners = std::sync::Arc::new(authority.tasks.clone());
                let runtime = self.app.tasks.clone();
                Box::pin(updates.filter_map(move |update| {
                    let owners = owners.clone();
                    let runtime = runtime.clone();
                    async move {
                        let snapshot = match update {
                            Ok(update) => update.snapshot,
                            Err(_) => return Some(Err(auth::unavailable())),
                        };
                        let owner = owners.get(&snapshot.task_id.to_string())?;
                        if snapshot.server != "computers" || snapshot.owner != *owner {
                            return Some(Err(auth::forbidden()));
                        }
                        Some(
                            veoveo_task_runtime::project_snapshot(&runtime, snapshot)
                                .await
                                .map_err(|_| auth::unavailable()),
                        )
                    }
                }))
            };
            // Establish the wake source before the baseline cursor. Each new listener
            // receives an invalidation baseline, then reads committed outbox pages.
            let platform = self.app.tasks.platform_store();
            let mut wake = platform
                .live::<OutboxEventRecord>(PlatformTable::OutboxEvent)
                .await
                .map_err(|_| auth::unavailable())?;
            let mut cursor = platform
                .latest_outbox_sequence()
                .await
                .map_err(|_| auth::unavailable())?;
            for (id, owner) in &authority.tasks {
                let snapshot =
                    veoveo_task_runtime::authorized_snapshot(&self.app.tasks, owner, id).await?;
                let task = veoveo_task_runtime::project_snapshot(&self.app.tasks, snapshot)
                    .await
                    .map_err(|_| auth::unavailable())?;
                context
                    .sink()
                    .notify_task_status(task)
                    .await
                    .map_err(|_| auth::unavailable())?;
            }
            for uri in &uris {
                context
                    .sink()
                    .notify_resource_updated(uri.clone())
                    .await
                    .map_err(|_| auth::unavailable())?;
            }
            let mut replay = false;
            let mut health = self.app.capacity_health();
            let mut health_open = true;
            let mut availability = self.app.availability();
            loop {
                let health_deadline = tokio::time::Instant::from_std(
                    health.borrow().observed_at + Duration::from_secs(15),
                );
                tokio::select! {
                    biased;
                    _ = context.cancelled() => return Ok(()),
                    changed = health.changed(), if health_open => {
                        health_open = changed.is_ok();
                    }
                    _ = tokio::time::sleep_until(health_deadline),
                        if !matches!(availability,
                            veoveo_computers::api::CapacityAvailability::SetupRequired |
                            veoveo_computers::api::CapacityAvailability::ComputeUnavailable) => {}
                    update = tasks.next() => {
                        let Some(update) = update else { return Err(auth::unavailable()); };
                        context.sink().notify_task_status(update?).await.map_err(|_| auth::unavailable())?;
                    }
                    event = wake.next(), if !replay => {
                        if !matches!(event, Some(Ok(_))) { return Err(auth::unavailable()); }
                        replay = true;
                    }
                    _ = async {}, if replay => {
                        let page = platform.read_outbox(cursor, 100).await.map_err(|_| auth::unavailable())?;
                        replay = page.events.len() == 100;
                        for event in page.events {
                            let Some(id) = resources::canonical_uuid(&event.aggregate_id) else { continue; };
                            let id = if event.aggregate_type == "computer" {
                                id
                            } else if event.aggregate_type == "task" && event.event_type == "task.cancel_requested" {
                                // Paused maintenance may have no active worker to emit
                                // a Computer event. Current cancellation still changes
                                // its recovery projection and must invalidate the UI.
                                match self.app.store.maintenance(actor.owner(), id).await {
                                    Ok(operation) => operation.computer_id,
                                    Err(veoveo_computers::ComputerError::NotFound) => continue,
                                    Err(_) => return Err(auth::unavailable()),
                                }
                            } else { continue; };
                            match self.app.store.get(actor.owner(), id).await {
                                Ok(_) => {},
                                Err(veoveo_computers::ComputerError::NotFound) => continue,
                                Err(_) => return Err(auth::unavailable()),
                            }
                            for uri in &uris {
                                if matches!(resources::parse(uri), Some(ResourceId::Collection(_)))
                                    || resources::parse(uri) == Some(ResourceId::Computer(id))
                                    || resources::parse(uri) == Some(ResourceId::Access(id))
                                    || resources::parse(uri) == Some(ResourceId::Automation(id))
                                    || resources::parse(uri) == Some(ResourceId::Maintenance(id))
                                    || matches!(resources::parse(uri), Some(ResourceId::Grant(computer, _)) if computer == id) {
                                    context.sink().notify_resource_updated(uri.clone()).await.map_err(|_| auth::unavailable())?;
                                }
                            }
                        }
                        cursor = page.next_sequence;
                    }
                }
                let current = self.app.availability();
                if current != availability {
                    for uri in &uris {
                        context
                            .sink()
                            .notify_resource_updated(uri.clone())
                            .await
                            .map_err(|_| auth::unavailable())?;
                    }
                    availability = current;
                }
            }
        };
        super::guard::run(
            authority.deadline,
            Duration::from_secs(5),
            context.cancelled(),
            pump,
            || async {
                tokio::time::timeout(
                    Duration::from_secs(5),
                    self.validate_listener(context.request_context(), &uris, &task_ids),
                )
                .await
                .map_err(|_| auth::unavailable())?
                .map(|authority| authority.deadline)
            },
        )
        .await
    }
}
