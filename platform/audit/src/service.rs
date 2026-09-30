//! Gateway-owned sealing and retention with lease election and observable failure.
use crate::{
    integrity::AuditSigningKey,
    sealer::{AuditSealer, SealError},
};
use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;
use veoveo_platform_store::{PlatformStore, StoreError, audit::AuditSealLease};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditHealthState {
    Starting,
    Active,
    Standby,
    Recovering,
    Failed,
    Stopped,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct HealthUpdate {
    state: AuditHealthState,
    checked: Instant,
}
impl HealthUpdate {
    pub(crate) fn now(state: AuditHealthState) -> Self {
        Self {
            state,
            checked: Instant::now(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct AuditHealth(watch::Receiver<HealthUpdate>);
impl AuditHealth {
    pub fn ready(&self) -> bool {
        let health = *self.0.borrow();
        matches!(
            health.state,
            AuditHealthState::Active | AuditHealthState::Standby
        ) && health.checked.elapsed() < Duration::from_secs(20)
    }
    pub fn state(&self) -> AuditHealthState {
        self.0.borrow().state
    }
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.0.changed().await
    }
}
#[derive(Debug, thiserror::Error)]
pub enum AuditServiceError {
    #[error("audit retention exceeds the supported UTC date range")]
    InvalidRetention,
    #[error(transparent)]
    Export(#[from] crate::export::ExportError),
    #[error(transparent)]
    Seal(#[from] SealError),
    #[error("audit service worker stopped unexpectedly")]
    Worker,
    #[error("audit service shutdown deadline exceeded; sealing is incomplete")]
    ShutdownDeadline,
}
pub struct AuditService {
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<Result<(), AuditServiceError>>,
    health: AuditHealth,
}
impl AuditService {
    pub fn start(
        store: PlatformStore,
        key: Arc<AuditSigningKey>,
        retention_days: NonZeroU32,
        exports: crate::export::AuditExportConfig,
    ) -> Result<Self, AuditServiceError> {
        chrono::Utc::now()
            .checked_sub_signed(chrono::TimeDelta::days(i64::from(retention_days.get())))
            .ok_or(AuditServiceError::InvalidRetention)?;
        let exporter = crate::export::AuditExporter::new(exports)?;
        let (stop, shutdown) = watch::channel(false);
        let (health, receiver) = watch::channel(HealthUpdate::now(AuditHealthState::Starting));
        let task = tokio::spawn(async move {
            let result = supervise(store, key, retention_days, exporter, shutdown, &health).await;
            health.send_replace(HealthUpdate::now(if result.is_ok() {
                AuditHealthState::Stopped
            } else {
                AuditHealthState::Failed
            }));
            result
        });
        Ok(Self {
            stop,
            task,
            health: AuditHealth(receiver),
        })
    }
    pub fn health(&self) -> AuditHealth {
        self.health.clone()
    }
    /// Call after the request writer drains so its final committed records are sealed.
    pub async fn shutdown(mut self, timeout: Duration) -> Result<(), AuditServiceError> {
        self.stop.send_replace(true);
        match tokio::time::timeout(timeout, &mut self.task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(AuditServiceError::Worker),
            Err(_) => {
                self.task.abort();
                let _ = (&mut self.task).await;
                Err(AuditServiceError::ShutdownDeadline)
            }
        }
    }
}
impl Drop for AuditService {
    fn drop(&mut self) {
        self.stop.send_replace(true);
        if !self.task.is_finished() {
            tracing::error!("audit service dropped before its shutdown completed");
            self.task.abort();
        }
    }
}
async fn supervise(
    store: PlatformStore,
    key: Arc<AuditSigningKey>,
    retention_days: NonZeroU32,
    exporter: crate::export::AuditExporter,
    mut shutdown: watch::Receiver<bool>,
    health: &watch::Sender<HealthUpdate>,
) -> Result<(), AuditServiceError> {
    let sealer = AuditSealer::new(store.clone(), key);
    let maintenance = Maintenance {
        store,
        retention_days,
        destinations: exporter.destination_ids(),
        exporter,
    };
    let mut delay = Duration::from_secs(1);
    loop {
        let (pause, recovery_error) = match sealer.run(shutdown.clone(), health, &maintenance).await
        {
            Ok(()) => return Ok(()),
            Err(SealError::Store(StoreError::AuditLeaseBusy)) => {
                health.send_replace(HealthUpdate::now(AuditHealthState::Standby));
                delay = Duration::from_secs(1);
                (Duration::from_secs(2), None)
            }
            Err(
                error @ (SealError::Export(_)
                | SealError::Integrity(_)
                | SealError::Store(
                    StoreError::AuditIntegrity
                    | StoreError::AuditExportRejected
                    | StoreError::AuditChangefeedGap
                    | StoreError::AuditBatchLimit
                    | StoreError::AuditValidation(_),
                )),
            ) => {
                tracing::error!(error = %error, "audit integrity service failed; operator recovery required");
                return Err(error.into());
            }
            Err(error) => {
                health.send_replace(HealthUpdate::now(AuditHealthState::Recovering));
                tracing::warn!(error = %error, "audit service reconnecting from committed cursor");
                let pause = delay;
                delay = (delay * 2).min(Duration::from_secs(10));
                (pause, Some(error))
            }
        };
        let stopping = *shutdown.borrow();
        let stopping = stopping
            || tokio::select! {
                _ = tokio::time::sleep(pause) => false,
                result = shutdown.changed() => result.is_err() || *shutdown.borrow(),
            };
        if stopping {
            // A failed drain or a disconnected worker has not sealed its pending
            // records. Only an elected standby may stop without doing that work.
            return recovery_error.map_or(Ok(()), |error| Err(error.into()));
        }
    }
}

pub(crate) struct Maintenance {
    store: PlatformStore,
    retention_days: NonZeroU32,
    pub(crate) exporter: crate::export::AuditExporter,
    destinations: Vec<veoveo_audit_contract::AuditDestinationId>,
}
impl Maintenance {
    pub(crate) async fn run(&self, lease: &AuditSealLease) -> Result<(), SealError> {
        let cutoff = chrono::Utc::now()
            .checked_sub_signed(chrono::TimeDelta::days(i64::from(
                self.retention_days.get(),
            )))
            .ok_or(StoreError::AuditIntegrity)?;
        let work = async {
            let mut deleted = 0usize;
            while deleted < 128 {
                let blocks = self
                    .store
                    .audit_retention_candidates(cutoff, &self.destinations)
                    .await?;
                if blocks.is_empty() {
                    break;
                }
                for block in blocks {
                    match self
                        .store
                        .retain_audit_block(lease, &block, cutoff, &self.destinations)
                        .await
                    {
                        Ok(()) => deleted += 1,
                        Err(StoreError::AuditRetentionNotAdmitted) => {}
                        Err(error) => return Err(error),
                    }
                    if deleted == 128 {
                        break;
                    }
                }
            }
            let guards = self.store.prune_audit_download_windows().await?;
            tracing::info!(
                audit_retained_blocks_deleted = deleted,
                audit_window_guards_deleted = guards,
                "audit retention pass"
            );
            Ok::<(), StoreError>(())
        };
        match tokio::time::timeout(Duration::from_secs(2), work).await {
            Ok(result) => result.map_err(SealError::from),
            Err(_) => {
                tracing::warn!("audit retention pass reached its two-second work budget");
                Ok(())
            }
        }
    }
}
