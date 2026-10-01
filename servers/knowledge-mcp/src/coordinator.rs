//! One authenticated source connection lifetime. The host owns discovery and
//! reconnects; each invocation re-establishes listeners and reconciles Store.
use crate::{
    ServiceError,
    embed::Embeddings,
    index::Indexer,
    source::{ObservableSource, SourceListener},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use veoveo_knowledge_contract::{
    CollectionRegistration, GenerationId, GenerationSpec, KnowledgeError,
};
use veoveo_mcp_knowledge_extension::{ChangeSignal, Freshness};
use veoveo_platform_store::{
    PlatformStore,
    knowledge::{CoordinatorId, CoordinatorLease},
};
use veoveo_types::TenantId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatorState {
    Starting,
    Synchronizing,
    Ready(GenerationId),
    CatalogReady,
    Stopped,
    Failed,
}

pub struct Coordinator<'a, S, E> {
    pub store: &'a PlatformStore,
    pub source: &'a S,
    pub embeddings: &'a E,
}

struct Change {
    collection: usize,
    lost: bool,
}

impl<S: ObservableSource, E: Embeddings> Coordinator<'_, S, E> {
    /// Cancellation releases this worker's lease; errors hide mutable cached
    /// results until a subsequent authenticated run reconciles them. Dropping an
    /// unresponsive process is covered by the Store's 30-second lease expiry.
    pub async fn run(
        &self,
        tenant: &TenantId,
        registrations: &[CollectionRegistration],
        specification: &GenerationSpec,
        cancel: CancellationToken,
        status: watch::Sender<CoordinatorState>,
    ) -> Result<(), ServiceError> {
        status.send_replace(CoordinatorState::Starting);
        let result = self
            .run_owned(tenant, registrations, specification, &cancel, &status)
            .await;
        status.send_replace(if result.is_ok() {
            CoordinatorState::Stopped
        } else {
            CoordinatorState::Failed
        });
        result
    }

    async fn run_owned(
        &self,
        tenant: &TenantId,
        registrations: &[CollectionRegistration],
        specification: &GenerationSpec,
        cancel: &CancellationToken,
        status: &watch::Sender<CoordinatorState>,
    ) -> Result<(), ServiceError> {
        if registrations.is_empty()
            || registrations.len() > 1024
            || registrations.iter().any(|r| {
                matches!(
                    r.descriptor.freshness(),
                    Freshness::MaxAge { max_age_seconds: 0 }
                )
            })
        {
            return Err(KnowledgeError(
                "indexing requires 1..1024 collections with a positive freshness lifetime",
            )
            .into());
        }
        let lease = tokio::time::timeout(
            Duration::from_secs(10),
            self.store
                .claim_knowledge_coordinator(tenant, CoordinatorId::new()),
        )
        .await
        .map_err(|_| ServiceError::Deadline)??
        .ok_or(ServiceError::CoordinatorBusy)?;
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Ok(()),
            result = self.renew(&lease) => result,
            result = self.synchronize(&lease, registrations, specification, status) => result,
        };
        // Await cleanup even after cancellation. If storage is unavailable,
        // expiry hides mutable members without requiring this write to succeed.
        let released = tokio::time::timeout(
            Duration::from_secs(10),
            self.store.release_knowledge_coordinator(&lease),
        )
        .await
        .map_err(|_| ServiceError::Deadline)?
        .map_err(ServiceError::from);
        result.and(released)
    }

    async fn renew(&self, lease: &CoordinatorLease) -> Result<(), ServiceError> {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            tokio::time::timeout(
                Duration::from_secs(10),
                self.store.renew_knowledge_coordinator(lease),
            )
            .await
            .map_err(|_| ServiceError::Deadline)??;
        }
    }

    async fn synchronize(
        &self,
        lease: &CoordinatorLease,
        registrations: &[CollectionRegistration],
        specification: &GenerationSpec,
        status: &watch::Sender<CoordinatorState>,
    ) -> Result<(), ServiceError> {
        let mut watchers = JoinSet::new();
        let (sender, mut changes) = mpsc::channel(1024);
        let mut has_listeners = false;
        // Every listener is acknowledged before the first enumeration. A burst
        // queued during setup is processed before any generation can activate.
        tokio::time::timeout(Duration::from_secs(3600), async {
            for (index, registration) in registrations.iter().enumerate() {
                if registration.descriptor.change_signal() != ChangeSignal::Listen {
                    continue;
                }
                has_listeners = true;
                let mut listener = tokio::time::timeout(
                    Duration::from_secs(30),
                    self.source.listen(&registration.descriptor),
                )
                .await
                .map_err(|_| ServiceError::Deadline)??;
                let sender = sender.clone();
                watchers.spawn(async move {
                    loop {
                        let lost = listener.changed().await.is_err();
                        if sender
                            .send(Change {
                                collection: index,
                                lost,
                            })
                            .await
                            .is_err()
                            || lost
                        {
                            break;
                        }
                    }
                });
            }
            Ok::<_, ServiceError>(())
        })
        .await
        .map_err(|_| ServiceError::Deadline)??;

        let indexer = Indexer {
            store: self.store,
            lease,
            source: self.source,
            embeddings: self.embeddings,
        };
        indexer.validate_inputs(lease.tenant(), registrations, specification)?;
        let previous = self
            .store
            .active_knowledge_generation(lease.tenant())
            .await?;
        let reusable = match previous {
            Some(id) => {
                self.store
                    .knowledge_generation(lease.tenant(), id)
                    .await?
                    .as_ref()
                    == Some(specification)
            }
            None => false,
        };
        let generation = if reusable {
            previous.expect("checked active generation")
        } else {
            indexer
                .prepare(lease.tenant(), registrations, specification)
                .await?
        };
        let mut activated = reusable;
        let mut pending: BTreeSet<usize> = (0..registrations.len()).collect();
        let mut due = BTreeMap::new();
        let mut sync_deadline = Instant::now() + Duration::from_secs(3600);

        loop {
            // Coalesce repeated invalidations in a bounded queue before work.
            for _ in 0..1024 {
                let Ok(change) = changes.try_recv() else {
                    break;
                };
                if Instant::now() >= sync_deadline {
                    return Err(ServiceError::Deadline);
                }
                self.invalidate(lease, registrations, generation, &mut pending, change)
                    .await?;
            }
            if let Some(index) = pending.pop_first() {
                status.send_replace(CoordinatorState::Synchronizing);
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep_until(sync_deadline) => return Err(ServiceError::Deadline),
                    change = changes.recv(), if has_listeners => {
                        pending.insert(index);
                        self.invalidate(lease, registrations, generation, &mut pending,
                            change.ok_or(ServiceError::SourceUnavailable)?).await?;
                    }
                    _ = watchers.join_next(), if has_listeners => return Err(ServiceError::SourceUnavailable),
                    result = indexer.reconcile(&registrations[index], generation, specification) => {
                        result?;
                        if let Freshness::MaxAge { max_age_seconds } = registrations[index].descriptor.freshness() {
                            due.insert(index, Instant::now() + Duration::from_millis(u64::from(max_age_seconds) * 500));
                        }
                    }
                }
                continue;
            }
            if !activated {
                self.store
                    .activate_knowledge_generation(lease, lease.tenant(), generation, previous)
                    .await?;
                activated = true;
                // Process events that arrived during activation before readiness.
                continue;
            }
            status.send_replace(CoordinatorState::Ready(generation));
            let next = due.values().copied().min();
            tokio::select! {
                biased;
                change = changes.recv(), if has_listeners => {
                    self.invalidate(lease, registrations, generation, &mut pending,
                        change.ok_or(ServiceError::SourceUnavailable)?).await?;
                }
                _ = watchers.join_next(), if has_listeners => return Err(ServiceError::SourceUnavailable),
                _ = async {
                    if let Some(next) = next { tokio::time::sleep_until(next).await; }
                    else { std::future::pending::<()>().await; }
                } => {
                    pending.extend(due.iter().filter_map(|(index, deadline)| (*deadline <= Instant::now()).then_some(*index)));
                }
            }
            sync_deadline = Instant::now() + Duration::from_secs(3600);
        }
    }

    async fn invalidate(
        &self,
        lease: &CoordinatorLease,
        registrations: &[CollectionRegistration],
        generation: GenerationId,
        pending: &mut BTreeSet<usize>,
        change: Change,
    ) -> Result<(), ServiceError> {
        self.store
            .invalidate_knowledge_collection(lease, &registrations[change.collection], generation)
            .await?;
        if change.lost {
            return Err(ServiceError::SourceUnavailable);
        }
        pending.insert(change.collection);
        Ok(())
    }
}
