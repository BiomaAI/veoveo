use super::{
    ComputersMcp, auth,
    resources::{self, ResourceId},
};
use futures::StreamExt;
use rmcp::{
    ErrorData, RoleServer,
    service::{RequestContext, SubscriptionContext},
};
use std::time::Duration;
use veoveo_computers::ComputerActor;
use veoveo_platform_store::{OutboxEventRecord, PlatformTable};
use veoveo_task_runtime::DurableTaskUpdateStream;

impl ComputersMcp {
    async fn validate_listener(
        &self,
        request: &RequestContext<RoleServer>,
        uris: &[String],
        tasks: &[String],
    ) -> Result<ComputerActor, ErrorData> {
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
                Some(ResourceId::Computer(id)) => {
                    control
                        .require_read(Some(id))
                        .map_err(|_| auth::forbidden())?;
                    self.app
                        .store
                        .get(actor.owner(), id)
                        .await
                        .map_err(|_| auth::forbidden())?;
                }
                _ => {
                    return Err(ErrorData::invalid_params(
                        "resource is not subscribable",
                        None,
                    ));
                }
            }
        }
        for id in tasks {
            let id = resources::canonical_uuid(id)
                .ok_or_else(|| ErrorData::invalid_params("unknown task", None))?;
            let operation = self
                .app
                .store
                .operation(actor.owner(), id)
                .await
                .map_err(|_| auth::forbidden())?;
            control
                .require_read(Some(operation.computer_id))
                .map_err(|_| auth::forbidden())?;
        }
        Ok(actor)
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
        let actor = tokio::time::timeout(
            Duration::from_secs(5),
            self.validate_listener(context.request_context(), &uris, &task_ids),
        )
        .await
        .map_err(|_| auth::unavailable())??;
        let remaining = (auth::identity(context.request_context())?.expires_at
            - chrono::Utc::now())
        .to_std()
        .map_err(|_| auth::forbidden())?;
        let pump = async {
            let mut tasks: DurableTaskUpdateStream = if task_ids.is_empty() {
                Box::pin(futures::stream::pending())
            } else {
                veoveo_task_runtime::subscribe_durable_tasks(
                    &self.app.tasks,
                    actor.owner().clone(),
                    task_ids.clone(),
                )
                .await?
                .updates
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
            for uri in &uris {
                context
                    .sink()
                    .notify_resource_updated(uri.clone())
                    .await
                    .map_err(|_| auth::unavailable())?;
            }
            let mut replay = false;
            loop {
                tokio::select! {
                    biased;
                    _ = context.cancelled() => return Ok(()),
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
                            if event.aggregate_type != "computer" { continue; }
                            let Some(id) = resources::canonical_uuid(&event.aggregate_id) else { continue; };
                            match self.app.store.get(actor.owner(), id).await {
                                Ok(_) => {},
                                Err(veoveo_computers::ComputerError::NotFound) => continue,
                                Err(_) => return Err(auth::unavailable()),
                            }
                            for uri in &uris {
                                if matches!(resources::parse(uri), Some(ResourceId::Collection(_))) || resources::parse(uri) == Some(ResourceId::Computer(id)) {
                                    context.sink().notify_resource_updated(uri.clone()).await.map_err(|_| auth::unavailable())?;
                                }
                            }
                        }
                        cursor = page.next_sequence;
                    }
                }
            }
        };
        tokio::pin!(pump);
        let expires = tokio::time::sleep(remaining);
        tokio::pin!(expires);
        let mut recheck = tokio::time::interval(Duration::from_secs(5));
        recheck.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        recheck.tick().await;
        loop {
            // The authority guard is outside the pump, including a blocked sink.
            tokio::select! {
                biased;
                _ = &mut expires => return Err(auth::forbidden()),
                _ = context.cancelled() => return Ok(()),
                result = &mut pump => return result,
                _ = recheck.tick() => {
                    tokio::time::timeout(Duration::from_secs(5), self.validate_listener(context.request_context(), &uris, &task_ids)).await.map_err(|_| auth::unavailable())??;
                }
            }
        }
    }
}
