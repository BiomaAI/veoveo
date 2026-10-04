//! Installation-owned machine connection, discovery and coordinator lifetimes.
use veoveo_gateway_contract::OAuthClientId;
mod connection;
mod control;

use crate::{
    ServiceError,
    coordinator::{Coordinator, CoordinatorState},
    embed::Embeddings,
    source::{GatewayCatalogListener, GatewaySource},
};
use futures::{StreamExt, stream::BoxStream};
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use veoveo_embedding_contract::EmbeddingTask;
use veoveo_knowledge_contract::{
    ChunkSettings, CollectionApproval, CollectionRegistration, GenerationSpec,
};
use veoveo_mcp_contract::JwtId;
use veoveo_platform_store::{PlatformStore, PlatformTable, ResourceInvalidation};
use veoveo_types::TenantId;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningAlgorithm {
    Rs256,
    Es256,
    EdDsa,
}

/// Credential files are read for each new connection, including scheduled rotation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexingConfig {
    pub tenant: TenantId,
    pub client_id: OAuthClientId,
    pub key_id: JwtId,
    pub signing_algorithm: SigningAlgorithm,
    pub private_key_file: PathBuf,
    pub trusted_ca_file: Option<PathBuf>,
    pub chunk_settings: ChunkSettings,
    pub query_task: EmbeddingTask,
}

pub struct IndexingService<'a, E> {
    pub store: &'a PlatformStore,
    pub catalog_registry: &'a veoveo_gateway_contract::CatalogRegistry,
    pub embeddings: &'a E,
    pub config: &'a IndexingConfig,
}

#[derive(Clone)]
pub struct IndexingReadiness(Vec<watch::Receiver<CoordinatorState>>);
impl IndexingReadiness {
    pub fn new(workers: Vec<watch::Receiver<CoordinatorState>>) -> Result<Self, ServiceError> {
        if workers.is_empty() || workers.len() > 128 {
            return Err(ServiceError::MachineConfiguration);
        }
        Ok(Self(workers))
    }
    pub fn is_ready(&self) -> bool {
        self.0.iter().all(|worker| {
            worker.has_changed().is_ok()
                && matches!(
                    *worker.borrow(),
                    CoordinatorState::Ready(_)
                        | CoordinatorState::Updating(_)
                        | CoordinatorState::CatalogReady
                )
        })
    }
}
struct Prepared {
    connection: connection::Connection,
    source: GatewaySource,
    catalog: GatewayCatalogListener,
    indexed: Vec<CollectionRegistration>,
    specification: Option<GenerationSpec>,
}
enum EpochEnd {
    Restart,
    Stop,
}

impl<E: Embeddings> IndexingService<'_, E> {
    pub async fn run(
        &self,
        cancel: CancellationToken,
        status: watch::Sender<CoordinatorState>,
    ) -> Result<(), ServiceError> {
        if self.config.chunk_settings.version() != crate::chunk::VERSION {
            return Err(ServiceError::MachineConfiguration);
        }
        let result = self.run_inner(&cancel, &status).await;
        status.send_replace(if result.is_ok() {
            CoordinatorState::Stopped
        } else {
            CoordinatorState::Failed
        });
        result
    }

    async fn run_inner(
        &self,
        cancel: &CancellationToken,
        status: &watch::Sender<CoordinatorState>,
    ) -> Result<(), ServiceError> {
        let mut controls = self
            .store
            .resource_changes(vec![PlatformTable::GatewayControlActive]);
        // The Store source yields a baseline only after installing native LIVE.
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            baseline = tokio::time::timeout(Duration::from_secs(30), controls.next()) => {
                baseline.map_err(|_| ServiceError::Deadline)?.ok_or(ServiceError::SourceUnavailable)?;
            }
        }
        let mut failures = 0u32;
        loop {
            status.send_replace(CoordinatorState::Starting);
            match self.epoch(&mut controls, cancel, status).await {
                Ok(EpochEnd::Stop) => return Ok(()),
                Ok(EpochEnd::Restart) => failures = 0,
                Err(error) => {
                    status.send_replace(CoordinatorState::Failed);
                    failures += 1;
                    if failures >= 8 {
                        return Err(error);
                    }
                    // Connection recovery is capped at eight failed epochs with
                    // 1,2,4,8,16,30,30-second delays; it never polls source state.
                    tracing::warn!(attempt = failures, error = %error, "knowledge source connection recovery");
                    tokio::select! {
                        _ = cancel.cancelled() => return Ok(()),
                        _ = tokio::time::sleep(Duration::from_secs((1u64 << (failures - 1)).min(30))) => {},
                        change = controls.next() => { change.ok_or(ServiceError::SourceUnavailable)?; failures = 0; },
                    }
                }
            }
        }
    }

    async fn prepare(&self) -> Result<Prepared, ServiceError> {
        let selected = control::select(self.store, self.config, self.catalog_registry).await?;
        let ticket = self
            .store
            .begin_knowledge_catalog(&self.config.tenant, &selected.discovery.control_revision)
            .await?;
        let connection = connection::connect(self.config, &selected).await?;
        let source = GatewaySource::from_authenticated_peer(connection.service.peer().clone());
        let discovered = source.discover(&selected.discovery).await?;
        self.store
            .replace_knowledge_catalog(ticket, &discovered.registrations)
            .await?;
        let indexed = discovered
            .registrations
            .into_iter()
            .filter(|registration| registration.approval.mode == CollectionApproval::Index)
            .collect::<Vec<_>>();
        let specification = if indexed.is_empty() {
            None
        } else {
            Some(GenerationSpec::new(
                self.embeddings.space().clone(),
                self.config.query_task.as_str(),
                self.config.chunk_settings.clone(),
                indexed
                    .iter()
                    .map(|registration| {
                        (
                            registration.descriptor.collection().clone(),
                            registration.revision(),
                        )
                    })
                    .collect(),
            )?)
        };
        Ok(Prepared {
            connection,
            source,
            catalog: discovered.listener,
            indexed,
            specification,
        })
    }

    async fn epoch(
        &self,
        controls: &mut BoxStream<'static, ResourceInvalidation>,
        cancel: &CancellationToken,
        status: &watch::Sender<CoordinatorState>,
    ) -> Result<EpochEnd, ServiceError> {
        let mut prepared = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(EpochEnd::Stop),
            change = controls.next() => { change.ok_or(ServiceError::SourceUnavailable)?; return Ok(EpochEnd::Restart); },
            prepared = tokio::time::timeout(Duration::from_secs(120), self.prepare()) => prepared.map_err(|_| ServiceError::Deadline)??,
        };
        let epoch_cancel = cancel.child_token();
        let coordinator = Coordinator {
            store: self.store,
            source: &prepared.source,
            embeddings: self.embeddings,
        };
        let drive = async {
            if let Some(specification) = &prepared.specification {
                coordinator
                    .run(
                        &self.config.tenant,
                        &prepared.indexed,
                        specification,
                        epoch_cancel.clone(),
                        status.clone(),
                    )
                    .await
            } else {
                status.send_replace(CoordinatorState::CatalogReady);
                epoch_cancel.cancelled().await;
                Ok(())
            }
        };
        tokio::pin!(drive);
        let mut ended = false;
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Ok(EpochEnd::Stop),
            change = controls.next() => change.ok_or(ServiceError::SourceUnavailable).map(|_| EpochEnd::Restart),
            _ = tokio::time::sleep_until(prepared.connection.rotate_at) => Ok(EpochEnd::Restart),
            changed = prepared.catalog.changed() => changed.map(|_| EpochEnd::Restart),
            result = &mut drive => { ended = true; result.and(Err(ServiceError::SourceUnavailable)) },
        };
        epoch_cancel.cancel();
        if !ended {
            tokio::time::timeout(Duration::from_secs(15), &mut drive)
                .await
                .map_err(|_| ServiceError::Deadline)??;
        }
        // Coordinator release and watcher shutdown precede connection replacement.
        let _ = prepared
            .connection
            .service
            .close_with_timeout(Duration::from_secs(5))
            .await;
        result
    }
}
