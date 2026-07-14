use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::Result;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind, SubscriptionHub};
use veoveo_platform_store::PrincipalKind as StorePrincipalKind;
use veoveo_task_runtime::{TaskOwner, TaskRuntime};

use crate::{
    acquisition::AcquisitionService,
    catalog::{TimeCatalog, TimeScope},
    clock::ClockMonitor,
    engine::TemporalEngine,
    registry::AuthorityRegistry,
};

#[derive(Clone)]
pub struct TimeApplication {
    pub tasks: TaskRuntime,
    pub catalog: TimeCatalog,
    pub authorities: AuthorityRegistry,
    pub clock: ClockMonitor,
    pub acquisitions: Arc<AcquisitionService>,
    pub subscriptions: Arc<SubscriptionHub>,
    pub activation: Arc<tokio::sync::Mutex<()>>,
    pub event_watchers:
        Arc<tokio::sync::Mutex<BTreeMap<String, tokio_util::sync::CancellationToken>>>,
}

impl TimeApplication {
    pub async fn scope(&self, identity: &GatewayInternalIdentity) -> Result<TimeScope> {
        let tenant_key = identity
            .principal
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "installation".to_owned());
        let platform_identity = self
            .tasks
            .platform_store()
            .ensure_identity(
                &tenant_key,
                identity.principal.id.as_str(),
                identity.principal.issuer.as_str(),
                identity.principal.subject.as_str(),
                match identity.principal.kind {
                    PrincipalKind::User => StorePrincipalKind::User,
                    PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
        Ok(TimeScope {
            identity: platform_identity,
        })
    }

    pub async fn scope_from_task_owner(&self, owner: &TaskOwner) -> Result<TimeScope> {
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
        Ok(TimeScope { identity })
    }

    pub async fn engine(&self, scope: &TimeScope) -> Result<TemporalEngine> {
        self.authorities.engine(&self.catalog, scope).await
    }

    pub async fn schedule_event(
        self: &Arc<Self>,
        scope: TimeScope,
        event: crate::contract::TemporalEvent,
    ) -> Result<()> {
        if event.state != crate::contract::TemporalEventState::Scheduled {
            return Ok(());
        }
        let engine = self.engine(&scope).await?;
        let now = engine
            .resolve(&crate::contract::ResolveTimeRequest {
                expression: crate::contract::TimeExpression::Rfc3339 {
                    value: chrono::Utc::now().to_rfc3339(),
                },
                additional_uncertainty_nanoseconds: 0,
            })?
            .instant;
        let delta = event.due.total_nanoseconds() - now.total_nanoseconds();
        let delay = if delta <= 0 {
            Duration::ZERO
        } else {
            Duration::from_nanos(u64::try_from(delta.min(i128::from(u64::MAX))).unwrap_or(u64::MAX))
        };
        let watcher_key = format!("{}:{}", scope.tenant_key(), event.event_id);
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
                            .notify_resource_updated(crate::uris::event_uri(current.event_id.as_str()))
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
        scope: &TimeScope,
        event_id: &crate::contract::TemporalEventId,
    ) {
        let key = format!("{}:{event_id}", scope.tenant_key());
        if let Some(cancellation) = self.event_watchers.lock().await.remove(&key) {
            cancellation.cancel();
        }
    }
}
