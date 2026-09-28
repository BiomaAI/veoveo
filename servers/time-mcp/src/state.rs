use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind, SubscriptionHub};
use veoveo_platform_store::PrincipalKind as StorePrincipalKind;
use veoveo_task_runtime::{TaskOwner, TaskRuntime};

use crate::{
    acquisition::AcquisitionService,
    catalog::{TimeAccessContext, TimeCatalog},
    clock::ClockMonitor,
    engine::TemporalEngine,
    registry::AuthorityRegistry,
};

type EventWatchers = BTreeMap<
    (
        veoveo_platform_store::TenantId,
        crate::contract::TemporalEventId,
    ),
    tokio_util::sync::CancellationToken,
>;

#[derive(Clone)]
pub struct TimeApplication {
    pub tasks: TaskRuntime,
    pub catalog: TimeCatalog,
    pub authorities: AuthorityRegistry,
    pub clock: ClockMonitor,
    pub acquisitions: Arc<AcquisitionService>,
    pub subscriptions: Arc<SubscriptionHub>,
    pub event_watchers: Arc<tokio::sync::Mutex<EventWatchers>>,
}

impl TimeApplication {
    pub async fn scope(&self, identity: &GatewayInternalIdentity) -> Result<TimeAccessContext> {
        let tenant_key = identity
            .actor
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "installation".to_owned());
        let platform_identity = self
            .tasks
            .platform_store()
            .ensure_identity(
                &tenant_key,
                identity.actor.id.as_str(),
                identity.actor.issuer.as_str(),
                identity.actor.subject.as_str(),
                match identity.actor.kind {
                    PrincipalKind::User => StorePrincipalKind::User,
                    PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
        Ok(TimeAccessContext {
            identity: platform_identity,
        })
    }

    pub async fn scope_from_task_owner(&self, owner: &TaskOwner) -> Result<TimeAccessContext> {
        let identity = self
            .tasks
            .platform_store()
            .ensure_identity(
                owner.tenant_key(),
                &owner.principal_key,
                &owner.issuer,
                &owner.subject,
                match owner.principal_kind {
                    veoveo_task_runtime::PrincipalKind::User => StorePrincipalKind::User,
                    veoveo_task_runtime::PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
        Ok(TimeAccessContext { identity })
    }

    pub async fn engine(&self, scope: &TimeAccessContext) -> Result<TemporalEngine> {
        self.authorities
            .authority_engine(&self.catalog, scope)
            .await
    }

    pub async fn engine_for_expressions<'a>(
        &self,
        scope: &TimeAccessContext,
        expressions: impl IntoIterator<Item = &'a crate::contract::TimeExpression>,
    ) -> Result<TemporalEngine> {
        let mut keys = BTreeSet::new();
        for (index, expression) in expressions.into_iter().enumerate() {
            anyhow::ensure!(
                index < 100_000,
                "at most 100000 temporal expressions are supported"
            );
            if let crate::contract::TimeExpression::EpochRelative { epoch_id, .. } = expression {
                keys.insert(epoch_id.clone());
            }
        }
        let keys: Vec<_> = keys.into_iter().collect();
        let engine = self.engine(scope).await?;
        let epochs = tokio::time::timeout(
            Duration::from_secs(30),
            self.catalog.epochs_for_keys(scope, &keys),
        )
        .await??;
        engine.replace_epochs(epochs);
        Ok(engine)
    }

    pub async fn restore_event_watchers(self: &Arc<Self>, scope: &TimeAccessContext) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(60), async {
            let mut after = None;
            loop {
                let page = self
                    .catalog
                    .events_page(
                        scope,
                        after.as_ref(),
                        Some(crate::contract::TemporalEventState::Scheduled),
                    )
                    .await?;
                self.schedule_events(scope.clone(), page.items).await?;
                after = page.next_cursor;
                if after.is_none() {
                    return Ok::<(), anyhow::Error>(());
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("temporal event recovery exceeded 60 seconds"))?
    }

    pub async fn schedule_event(
        self: &Arc<Self>,
        scope: TimeAccessContext,
        event: crate::contract::TemporalEvent,
    ) -> Result<()> {
        self.schedule_events(scope, [event]).await
    }

    /// A page of events shares one request-validated authority context.
    pub async fn schedule_events(
        self: &Arc<Self>,
        scope: TimeAccessContext,
        events: impl IntoIterator<Item = crate::contract::TemporalEvent>,
    ) -> Result<()> {
        let mut pending: Vec<_> = events
            .into_iter()
            .filter(|event| event.state == crate::contract::TemporalEventState::Scheduled)
            .collect();
        {
            let watchers = self.event_watchers.lock().await;
            pending.retain(|event| {
                !watchers.contains_key(&(scope.identity.tenant_id, event.event_id.clone()))
            });
        }
        if pending.is_empty() {
            return Ok(());
        }
        let engine = self.engine(&scope).await?;
        for event in pending {
            self.schedule_with_engine(scope.clone(), event, &engine)
                .await?;
        }
        Ok(())
    }

    async fn schedule_with_engine(
        self: &Arc<Self>,
        scope: TimeAccessContext,
        event: crate::contract::TemporalEvent,
        engine: &TemporalEngine,
    ) -> Result<()> {
        let now = engine
            .resolve(&crate::contract::ResolveTimeRequest {
                expression: crate::contract::TimeExpression::Rfc3339 {
                    value: chrono::Utc::now().to_rfc3339(),
                },
                additional_uncertainty_nanoseconds: 0,
            })?
            .into_instant();
        let delta = event.due.total_nanoseconds() - now.total_nanoseconds();
        let delay = if delta <= 0 {
            Duration::ZERO
        } else {
            Duration::from_nanos(u64::try_from(delta.min(i128::from(u64::MAX))).unwrap_or(u64::MAX))
        };
        let watcher_key = (scope.identity.tenant_id, event.event_id.clone());
        let cancellation = tokio_util::sync::CancellationToken::new();
        {
            let mut watchers = self.event_watchers.lock().await;
            if watchers.contains_key(&watcher_key) {
                return Ok(());
            }
            watchers.insert(watcher_key.clone(), cancellation.clone());
        }
        let state = self.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(delay) => {
                    if let Ok(Some(current)) = state.catalog.event(&scope, &event.event_id).await
                        && current.state == crate::contract::TemporalEventState::Scheduled
                        && state
                            .catalog
                            .mark_event_due(&scope, &current.event_id, current.record_version)
                            .await
                            .is_ok()
                    {
                        state
                            .subscriptions
                            .notify_resource_updated(crate::uris::EVENTS_URI)
                            .await;
                        state
                            .subscriptions
                            .notify_resource_updated(crate::contract::TimeResource::Event(current.event_id.clone()).to_string())
                            .await;
                    }
                }
                () = cancellation.cancelled() => {}
            }
            state.event_watchers.lock().await.remove(&watcher_key);
        });
        Ok(())
    }

    pub async fn cancel_event_watcher(
        &self,
        scope: &TimeAccessContext,
        event_id: &crate::contract::TemporalEventId,
    ) {
        let key = (scope.identity.tenant_id, event_id.clone());
        if let Some(cancellation) = self.event_watchers.lock().await.remove(&key) {
            cancellation.cancel();
        }
    }
}
