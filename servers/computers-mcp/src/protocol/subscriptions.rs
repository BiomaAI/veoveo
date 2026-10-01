use super::{ComputersMcp, auth};
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
use veoveo_computers_contract::ComputerResource;
use veoveo_platform_store::{ChangefeedDelivery, ComputerChange, PlatformTable};
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
        let mut deadline = control.valid_until();
        for uri in uris {
            match ComputerResource::parse(uri).ok() {
                Some(ComputerResource::Collection(_)) => {
                    control.require_read(None).map_err(|_| auth::forbidden())?
                }
                Some(ComputerResource::Computer(id)) => {
                    let access = self
                        .app
                        .store
                        .read_computer_access(&actor, &control, id)
                        .await
                        .map_err(|_| auth::forbidden())?;
                    deadline = deadline.min(access.valid_until());
                }
                Some(
                    ComputerResource::Access(id)
                    | ComputerResource::Automation(id)
                    | ComputerResource::Maintenance(id),
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
                Some(ComputerResource::Grant { computer, grant }) => {
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
        let changes = self
            .app
            .store
            .authority_changes()
            .await
            .map_err(|_| auth::unavailable())?
            .into_stream(veoveo_computers::AuthorityInterest::All);
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
                let mut streams = Vec::new();
                for (id, owner) in &authority.tasks {
                    let admitted = veoveo_task_runtime::subscribe_durable_tasks(
                        &self.app.tasks.for_owner(owner),
                        vec![id.clone()],
                    )
                    .await?;
                    if admitted.accepted_task_ids.len() != 1 {
                        return Err(auth::forbidden());
                    }
                    streams.push(admitted.updates);
                }
                Box::pin(futures::stream::select_all(streams))
            };
            let platform = self.app.tasks.platform_store();
            let baseline_cursor = platform
                .changefeed_head()
                .await
                .map_err(|_| auth::unavailable())?;
            let mut wake = platform.observe_changes(
                vec![
                    PlatformTable::Computer,
                    PlatformTable::ComputerAutomationGrant,
                    PlatformTable::ComputerSessionGrant,
                    PlatformTable::ComputerCliGrant,
                    PlatformTable::ComputerMaintenance,
                    PlatformTable::Task,
                ],
                baseline_cursor,
            );
            // Install LIVE before reading the initial resource state.
            wake.next()
                .await
                .ok_or_else(auth::unavailable)?
                .map_err(|_| auth::unavailable())?;
            for (id, owner) in &authority.tasks {
                let snapshot =
                    veoveo_task_runtime::authorized_snapshot(&self.app.tasks.for_owner(owner), id)
                        .await?;
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
                    event = wake.next() => {
                        let delivery = event.ok_or_else(auth::unavailable)?.map_err(|_| auth::unavailable())?;
                        let ChangefeedDelivery::Changes { entries, .. } = delivery else {
                            // Reattachment must establish a fresh authority baseline.
                            return Err(auth::unavailable());
                        };
                        for entry in entries {
                            let change = ComputerChange::decode(&entry).map_err(|_| auth::unavailable())?;
                            let (id, grant) = match change {
                                Some(ComputerChange::Computer(id)) => (id, None),
                                Some(ComputerChange::Automation { computer, grant }) => (computer, Some(grant)),
                                Some(ComputerChange::Task(task)) => {
                                    // Any Task mutation can alter maintenance recovery.
                                    match self.app.store.maintenance(actor.owner(), task.as_uuid()).await {
                                        Ok(operation) => (operation.computer_id, None),
                                        Err(veoveo_computers::ComputerError::NotFound) => continue,
                                        Err(_) => return Err(auth::unavailable()),
                                    }
                                }
                                None => continue,
                            };
                            let control = self.app.store.control_authority(actor).await
                                .map_err(|_| auth::forbidden())?;
                            match self.app.store.read_computer_access(actor, &control, id).await {
                                Ok(_) => {},
                                Err(veoveo_computers::ComputerError::NotFound) => {
                                    // A revoked Read grant must remove the row from its
                                    // recipient's view. Send only a collection hint,
                                    // never the revoked Computer or grant contents.
                                    {
                                        if let Some(grant) = grant
                                            && self.app.store.automation_change_recipient(actor, &control, id, grant)
                                                .await.map_err(|_| auth::unavailable())? {
                                            for uri in &uris {
                                                if matches!(ComputerResource::parse(uri).ok(), Some(ComputerResource::Collection(_))) {
                                                    context.sink().notify_resource_updated(uri.clone()).await.map_err(|_| auth::unavailable())?;
                                                }
                                            }
                                        }
                                    }
                                    continue;
                                },
                                Err(_) => return Err(auth::unavailable()),
                            }
                            for uri in &uris {
                                if matches!(ComputerResource::parse(uri).ok(), Some(ComputerResource::Collection(_)))
                                    || ComputerResource::parse(uri).ok() == Some(ComputerResource::Computer(id))
                                    || ComputerResource::parse(uri).ok() == Some(ComputerResource::Access(id))
                                    || ComputerResource::parse(uri).ok() == Some(ComputerResource::Automation(id))
                                    || ComputerResource::parse(uri).ok() == Some(ComputerResource::Maintenance(id))
                                    || matches!(ComputerResource::parse(uri).ok(), Some(ComputerResource::Grant { computer, .. }) if computer == id) {
                                    context.sink().notify_resource_updated(uri.clone()).await.map_err(|_| auth::unavailable())?;
                                }
                            }
                        }
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
            changes,
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
