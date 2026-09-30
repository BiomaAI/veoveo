//! One installation sealer follows committed changes under a database-fenced lease.
use crate::integrity::{AuditSigningKey, IntegrityError};
use crate::service::{AuditHealthState, HealthUpdate, Maintenance};
use futures::StreamExt;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use veoveo_audit_contract::*;
use veoveo_platform_store::{PlatformStore, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Integrity(#[from] IntegrityError),
    #[error(transparent)]
    Export(#[from] crate::export::ExportError),
    #[error("audit live stream disconnected; recovery requires reconnecting")]
    Disconnected,
}
pub struct AuditSealer {
    store: PlatformStore,
    key: Arc<AuditSigningKey>,
    owner: uuid::Uuid,
}
impl AuditSealer {
    pub fn new(store: PlatformStore, key: Arc<AuditSigningKey>) -> Self {
        Self {
            store,
            key,
            owner: uuid::Uuid::now_v7(),
        }
    }
    pub async fn seal_pending(&self) -> Result<usize, SealError> {
        Ok(self.seal_page().await?.0)
    }
    async fn seal_page(&self) -> Result<(usize, bool), SealError> {
        let lease = self.store.acquire_audit_seal_lease(self.owner).await?;
        let committed = self.store.audit_committed_records(&lease).await?;
        let count = committed.records.len();
        let mut partitions =
            BTreeMap::<AuditPartition, Vec<(AuditVersionstamp, AuditRecord)>>::new();
        for (stamp, record) in committed.records {
            partitions
                .entry(record.draft.partition().clone())
                .or_default()
                .push((stamp, record));
        }
        let mut blocks = Vec::with_capacity(partitions.len());
        let now = chrono::Utc::now();
        for (partition, records) in partitions {
            let previous = self.store.audit_partition_checkpoint(&partition).await?;
            let members = records
                .iter()
                .map(|(versionstamp, record)| AuditBlockMember {
                    id: record.draft.id(),
                    versionstamp: *versionstamp,
                })
                .collect();
            let records = records
                .into_iter()
                .map(|(_, record)| record)
                .collect::<Vec<_>>();
            blocks.push(
                self.key
                    .seal(partition, previous.as_ref(), members, &records, now)?,
            );
        }
        self.store
            .commit_audit_blocks(&lease, committed.next, &blocks)
            .await?;
        tracing::info!(
            audit_sealed_records = count,
            audit_sealed_blocks = blocks.len(),
            "audit seal committed"
        );
        Ok((count, committed.next.get() > lease.cursor as u64))
    }
    /// Native LIVE is the wake source. The one-second timer coalesces seal work;
    /// an idle timer never polls records. Lease renewal has a separate 10 s clock.
    /// A stream disconnect exits so the gateway supervisor can reconnect and replay
    /// the persisted cursor. Integrity failures must make that supervisor unhealthy.
    pub(crate) async fn run(
        &self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
        health: &tokio::sync::watch::Sender<HealthUpdate>,
        maintenance: &Maintenance,
    ) -> Result<(), SealError> {
        let mut lease = self.store.acquire_audit_seal_lease(self.owner).await?;
        health.send_replace(HealthUpdate::now(AuditHealthState::Recovering));
        let mut wake = self.store.audit_sealer_wakes().await?;
        let export_lease = lease.clone();
        let export = maintenance.exporter.run(&self.store, &export_lease);
        tokio::pin!(export);
        let mut flush = tokio::time::interval(Duration::from_secs(1));
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut renew = tokio::time::interval(Duration::from_secs(10));
        renew.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut maintain = tokio::time::interval(Duration::from_secs(60));
        maintain.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut pending = true;
        let mut sealed_once = false;
        loop {
            if *shutdown.borrow() {
                self.drain(&lease).await?;
                return Ok(());
            }
            tokio::select! {
                result = &mut export => {
                    return Err(match result {
                        Err(crate::export::ExportError::Store(error)) => SealError::Store(error),
                        Err(crate::export::ExportError::Disconnected) => SealError::Disconnected,
                        Err(error) => error.into(),
                        Ok(()) => SealError::Disconnected,
                    });
                }
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow() {
                        self.drain(&lease).await?;
                        return Ok(());
                    }
                }
                next = wake.next() => match next {
                    Some(Ok(())) => pending = true,
                    Some(Err(error)) => return Err(error.into()),
                    None => return Err(SealError::Disconnected),
                },
                _ = renew.tick() => {
                    lease = self.store.renew_audit_seal_lease(&lease, !pending).await?;
                    if sealed_once { health.send_replace(HealthUpdate::now(AuditHealthState::Active)); }
                }
                _ = maintain.tick() => {
                    maintenance.run(&lease).await?;
                }
                _ = flush.tick(), if pending => {
                    // A zero-record page may contain unrelated table changes. Keep
                    // draining until Store reports that the database cursor is idle.
                    pending = self.seal_page().await?.1;
                    sealed_once = true;
                    health.send_replace(HealthUpdate::now(AuditHealthState::Active));
                }
            }
        }
    }

    async fn drain(
        &self,
        lease: &veoveo_platform_store::audit::AuditSealLease,
    ) -> Result<(), SealError> {
        // Producers are stopped before shutdown. Schema and unrelated domain
        // pages still advance the cursor and must not end the drain early.
        while self.seal_page().await?.1 {}
        self.store.release_audit_seal_lease(lease).await?;
        Ok(())
    }
}
