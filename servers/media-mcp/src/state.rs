//! Durable media task state backed by the installation SurrealDB.

use crate::storage::{MediaTaskContextId, MediaTaskContextRecord};
mod cancellation;
mod usage;
use cancellation::CancellationReceiptRecord;
pub use usage::MediaBillingPage;

use std::collections::BTreeSet;
use veoveo_platform_store::task_record_id;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use veoveo_mcp_contract::{
    ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret, IssuedArtifactWriteCapability,
};
use veoveo_platform_store::{
    ArtifactWriteCapabilityId as StoreCapabilityId, OpenObject, PlatformStore, ProviderEventId,
    ProviderJobId, ProviderJobRecord, ProviderJobState, RecordId, RecordIdKey, RedactedSecret,
    StoreError, TaskStatus,
};
use veoveo_task_runtime::{TaskFailure, TaskOwner, TaskRuntime, TaskSnapshot};
use veoveo_types::DataLabelId;
use veoveo_types::TaskId;

use crate::{
    contract::{MediaPredictionId, MediaPredictionUri},
    provider::Prediction,
};

const PROVIDER: &str = "media";

const STATE_ID_NAMESPACE: Uuid = Uuid::from_u128(0xc05a_75ed_011f_5234_9482_9e94_be0c_1cc1);

#[derive(Clone)]
pub struct MediaState {
    store: PlatformStore,
}

#[derive(Clone)]
pub struct MediaTaskContext {
    pub task_id: TaskId,
    pub owner: TaskOwner,
    pub artifact_write_capability: IssuedArtifactWriteCapability,
}

impl std::fmt::Debug for MediaTaskContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MediaTaskContext")
            .field("task_id", &self.task_id)
            .field("owner", &self.owner)
            .field("artifact_write_capability", &self.artifact_write_capability)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct MediaProviderJob {
    pub job_id: ProviderJobId,
    pub task_id: TaskId,
    pub external_job_id: MediaPredictionId,
    pub state: ProviderJobState,
    pub prediction: Prediction,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct MediaProviderEvent {
    pub event_id: ProviderEventId,
    pub webhook_id: String,
    pub job: MediaProviderJob,
    pub prediction: Prediction,
    pub processed_at: Option<DateTime<Utc>>,
    receipt: veoveo_task_runtime::AuthenticatedWebhookReceipt,
    pub authoritative: bool,
}

#[derive(Clone, Debug)]
pub struct WebhookReceipt {
    pub event: MediaProviderEvent,
    pub inserted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ProviderCancellationOutcome {
    Requested,
    Accepted { deleted_count: u64 },
    NotDeleted { deleted_count: u64 },
    Failed { error: String },
}

impl MediaState {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &PlatformStore {
        &self.store
    }

    pub async fn persist_task_context(
        &self,
        snapshot: &TaskSnapshot,
        capability: &IssuedArtifactWriteCapability,
    ) -> Result<MediaTaskContext, StoreError> {
        self.persist_preallocated_task_context(snapshot.task_id, &snapshot.owner, capability)
            .await?;
        self.task_context(snapshot)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "media task context readback",
            })
    }

    /// Persist the private completion context before publishing the task row.
    /// A crash may leave an expiring orphan context, but can never leave a
    /// visible task without the capability required by a later webhook.
    pub async fn persist_preallocated_task_context(
        &self,
        task_id: TaskId,
        owner: &TaskOwner,
        capability: &IssuedArtifactWriteCapability,
    ) -> Result<(), StoreError> {
        if capability.task_id != task_id.to_string() {
            return Err(StoreError::ArtifactWriteConflict {
                key: task_id.to_string(),
            });
        }
        let context_id = MediaTaskContextId::from_uuid(task_id.as_uuid());
        let now = Utc::now();
        let content = MediaTaskContextRecord {
            id: context_id.record_id(),
            task: task_record_id(task_id),
            tenant: tenant_record(owner)?,
            capability: StoreCapabilityId::from_uuid(capability.capability_id.as_uuid())
                .record_id(),
            capability_secret: RedactedSecret::new(capability.secret.expose_secret()),
            capability_expires_at: capability.expires_at,
            created_at: now,
            updated_at: now,
        };

        let result = self
            .store
            .client()
            .query(include_str!("queries/create_task_context.surql"))
            .bind(("context", context_id.record_id()))
            .bind(("content", content))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result
            && self.task_context_record(task_id).await?.is_none()
        {
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn task_context(
        &self,
        snapshot: &TaskSnapshot,
    ) -> Result<Option<MediaTaskContext>, StoreError> {
        let Some(context) = self.task_context_record(snapshot.task_id).await? else {
            return Ok(None);
        };
        let capability_id = record_uuid(&context.capability)?;
        let capability = IssuedArtifactWriteCapability {
            capability_id: ArtifactWriteCapabilityId::parse(capability_id.to_string()).map_err(
                |_| StoreError::MissingRecord {
                    operation: "media task capability identity",
                },
            )?,
            secret: ArtifactWriteCapabilitySecret::new(context.capability_secret.expose_secret())
                .map_err(|_| StoreError::MissingRecord {
                operation: "media task capability secret",
            })?,
            task_id: snapshot.task_id.to_string(),
            expires_at: context.capability_expires_at,
        };
        Ok(Some(MediaTaskContext {
            task_id: snapshot.task_id,
            owner: snapshot.owner.clone(),
            artifact_write_capability: capability,
        }))
    }

    async fn task_context_record(
        &self,
        task_id: TaskId,
    ) -> Result<Option<MediaTaskContextRecord>, StoreError> {
        let context_id = MediaTaskContextId::from_uuid(task_id.as_uuid());
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/read_task_context.surql"))
            .bind(("context", context_id.record_id()))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }

    fn journal(&self) -> Result<veoveo_task_runtime::WebhookJournal, StoreError> {
        let runtime = crate::task_lookup::bind(TaskRuntime::new(
            self.store.clone(),
            "media",
            "media-journal",
        ))
        .map_err(task_store_error)?;
        Ok(runtime
            .webhooks(veoveo_types::ExtensionName::parse(PROVIDER).expect("declared provider")))
    }

    async fn check_prediction_request(
        &self,
        runtime: &TaskRuntime,
        task_id: TaskId,
        prediction: &Prediction,
    ) -> Result<(), StoreError> {
        let current = runtime
            .get(task_id)
            .await
            .map_err(task_store_error)?
            .ok_or(StoreError::MissingRecord {
                operation: "media prediction Task",
            })?;
        let request: crate::contract::RunArgs =
            serde_json::from_value(current.request).map_err(|_| StoreError::MissingRecord {
                operation: "media prediction request",
            })?;
        if request.model != prediction.model {
            return Err(StoreError::ArtifactWriteConflict {
                key: task_id.to_string(),
            });
        }
        Ok(())
    }

    pub async fn bind_submission_and_wait(
        &self,
        runtime: &TaskRuntime,
        task_id: TaskId,
        prediction: &Prediction,
    ) -> Result<MediaProviderJob, StoreError> {
        self.check_prediction_request(runtime, task_id, prediction)
            .await?;
        let journal = runtime
            .webhooks(veoveo_types::ExtensionName::parse(PROVIDER).expect("declared provider"));
        let external = veoveo_platform_store::ProviderJobKey::parse(prediction.id.to_string())
            .map_err(|_| StoreError::MissingRecord {
                operation: "media provider identity",
            })?;
        let record = journal
            .bind_submission(
                task_id,
                external,
                crate::storage::PredictionRecord(prediction.clone()),
                format!(
                    "submitted; prediction {}; resource {}; waiting for signed provider webhook",
                    prediction.id,
                    MediaPredictionUri::new(prediction.id.clone())
                ),
            )
            .await
            .map_err(task_store_error)?;
        provider_job(record)
    }

    pub async fn receive_webhook(
        &self,
        runtime: &TaskRuntime,
        task_id: TaskId,
        webhook_id: &str,
        prediction: &Prediction,
    ) -> Result<WebhookReceipt, StoreError> {
        self.check_prediction_request(runtime, task_id, prediction)
            .await?;
        let terminal = match prediction.terminal_outcome() {
            Some(crate::provider::TerminalOutcome::Succeeded) => {
                Some(veoveo_task_runtime::WebhookTerminal::Succeeded)
            }
            Some(crate::provider::TerminalOutcome::Cancelled) => {
                Some(veoveo_task_runtime::WebhookTerminal::Cancelled)
            }
            Some(
                crate::provider::TerminalOutcome::Failed
                | crate::provider::TerminalOutcome::TimedOut
                | crate::provider::TerminalOutcome::Deleted,
            ) => Some(veoveo_task_runtime::WebhookTerminal::Failed),
            None => None,
        };
        let external = veoveo_platform_store::ProviderJobKey::parse(prediction.id.to_string())
            .map_err(|_| StoreError::MissingRecord {
                operation: "media provider identity",
            })?;
        let event = veoveo_platform_store::ProviderEventKey::parse(webhook_id).map_err(|_| {
            StoreError::MissingRecord {
                operation: "media provider event identity",
            }
        })?;
        let receipt = runtime
            .webhooks(veoveo_types::ExtensionName::parse(PROVIDER).expect("declared provider"))
            .receive_authenticated(
                task_id,
                external,
                event,
                terminal,
                crate::storage::PredictionRecord(prediction.clone()),
                None,
            )
            .await
            .map_err(task_store_error)?;
        let inserted = receipt.inserted;
        Ok(WebhookReceipt {
            event: media_event(receipt)?,
            inserted,
        })
    }

    pub async fn pending_events(
        &self,
        limit: usize,
    ) -> Result<Vec<MediaProviderEvent>, StoreError> {
        use veoveo_types::TaskTypeDefinition;
        self.journal()?
            .pending(&[crate::contract::MediaTaskKind::Run.name()], limit)
            .await
            .map_err(task_store_error)?
            .into_iter()
            .map(media_event)
            .collect()
    }

    pub async fn complete_event(
        &self,
        runtime: &TaskRuntime,
        event: &MediaProviderEvent,
        result: Result<Value, TaskFailure>,
        message: String,
    ) -> Result<TaskSnapshot, StoreError> {
        let current = runtime
            .get(event.job.task_id)
            .await
            .map_err(task_store_error)?
            .ok_or(StoreError::MissingRecord {
                operation: "media webhook Task",
            })?;
        let journal = runtime
            .webhooks(veoveo_types::ExtensionName::parse(PROVIDER).expect("declared provider"));
        if current.is_terminal() {
            journal
                .acknowledge(&event.receipt)
                .await
                .map_err(task_store_error)?;
            return Ok(current);
        }
        let transition = match result {
            Ok(value) => veoveo_task_runtime::mcp_task_completion(
                message,
                serde_json::from_value(value).map_err(json_store_error)?,
            )
            .map_err(task_store_error)?,
            Err(failure) => veoveo_task_runtime::TaskTransition::Failed(failure),
        };
        journal
            .settle(&event.receipt, transition)
            .await
            .map_err(task_store_error)
    }

    pub async fn acknowledge_cancelled_event(
        &self,
        runtime: &TaskRuntime,
        event: &MediaProviderEvent,
    ) -> Result<TaskSnapshot, StoreError> {
        let current = runtime
            .get(event.job.task_id)
            .await
            .map_err(task_store_error)?
            .ok_or(StoreError::MissingRecord {
                operation: "media cancelled webhook Task",
            })?;
        if !matches!(
            current.status,
            TaskStatus::CancelRequested | TaskStatus::Cancelled
        ) {
            return Err(StoreError::ArtifactWriteConflict {
                key: current.task_id.to_string(),
            });
        }
        runtime
            .webhooks(veoveo_types::ExtensionName::parse(PROVIDER).expect("declared provider"))
            .settle(
                &event.receipt,
                veoveo_task_runtime::TaskTransition::Cancelled,
            )
            .await
            .map_err(task_store_error)
    }

    pub async fn acknowledge_superseded_event(
        &self,
        event: &MediaProviderEvent,
    ) -> Result<(), StoreError> {
        self.journal()?
            .acknowledge(&event.receipt)
            .await
            .map_err(task_store_error)
    }

    pub async fn record_processing_error(
        &self,
        event: &MediaProviderEvent,
        error: &str,
    ) -> Result<(), StoreError> {
        self.journal()?
            .processing_error(&event.receipt, truncate(error, 2000))
            .await
            .map_err(task_store_error)
    }

    pub async fn provider_job_for_task(
        &self,
        task: TaskId,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        self.journal()?
            .job_for_task(task)
            .await
            .map_err(task_store_error)?
            .map(provider_job)
            .transpose()
    }

    pub async fn record_provider_cancellation(
        &self,
        task: &TaskSnapshot,
        job: &MediaProviderJob,
        outcome: ProviderCancellationOutcome,
    ) -> Result<MediaProviderJob, StoreError> {
        if job.task_id != task.task_id {
            return Err(StoreError::ArtifactWriteConflict {
                key: task.task_id.to_string(),
            });
        }
        let receipt = CancellationReceiptRecord {
            recorded_at: Utc::now(),
            result: outcome,
        }
        .into_payload();
        provider_job(
            self.journal()?
                .record_cancellation(task.task_id, receipt)
                .await
                .map_err(task_store_error)?,
        )
    }

    pub async fn provider_job_for_task_prediction(
        &self,
        task: TaskId,
        prediction: &MediaPredictionId,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        let job = self.provider_job_for_task(task).await?;
        Ok(job.filter(|job| &job.external_job_id == prediction))
    }

    pub async fn prune_task_contexts(&self) -> Result<u64, StoreError> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/expire_task_contexts.surql"))
            .await?
            .check()?;
        Ok(response.take::<Vec<MediaTaskContextRecord>>(0)?.len() as u64)
    }
}

fn media_event(
    receipt: veoveo_task_runtime::AuthenticatedWebhookReceipt,
) -> Result<MediaProviderEvent, StoreError> {
    Ok(MediaProviderEvent {
        event_id: ProviderEventId::from_uuid(record_uuid(&receipt.event.id)?),
        webhook_id: receipt.event.event_id.clone(),
        job: provider_job(receipt.job.clone())?,
        prediction: prediction_from_payload(receipt.event.payload.clone())?,
        processed_at: receipt.event.processed_at,
        authoritative: receipt.authoritative,
        receipt,
    })
}

fn provider_job(record: ProviderJobRecord) -> Result<MediaProviderJob, StoreError> {
    Ok(MediaProviderJob {
        job_id: ProviderJobId::from_uuid(record_uuid(&record.id)?),
        task_id: TaskId::from_uuid(record_uuid(&record.task)?),
        external_job_id: MediaPredictionId::new(record.external_job_id).map_err(|_| {
            StoreError::MissingRecord {
                operation: "media prediction identity",
            }
        })?,
        state: record.state,
        prediction: prediction_from_payload(record.provider_payload)?,
        updated_at: record.updated_at,
    })
}

fn prediction_from_payload(payload: OpenObject) -> Result<Prediction, StoreError> {
    crate::storage::PredictionRecord::from_payload(payload)
        .map(|record| record.0)
        .map_err(json_store_error)
}

fn tenant_record(owner: &TaskOwner) -> Result<RecordId, StoreError> {
    veoveo_platform_store::deterministic_tenant_id(owner.tenant_key()).map(|id| id.record_id())
}

fn open_object(value: Value) -> OpenObject {
    let Value::Object(values) = value else {
        unreachable!("typed Media envelopes serialize as objects");
    };
    OpenObject::new(values.into_iter().collect())
}

fn record_uuid(record: &RecordId) -> Result<Uuid, StoreError> {
    match &record.key {
        RecordIdKey::Uuid(value) => Ok(**value),
        _ => Err(StoreError::MissingRecord {
            operation: "media UUID record decoding",
        }),
    }
}

fn task_store_error(error: veoveo_task_runtime::TaskError) -> StoreError {
    match error {
        veoveo_task_runtime::TaskError::Database(error) => StoreError::Database(error),
        veoveo_task_runtime::TaskError::Store(error) => error,
        veoveo_task_runtime::TaskError::NotFound(_) => StoreError::AdministrationFailed {
            operation: "media task not found",
        },
        _ => StoreError::AdministrationFailed {
            operation: "media durable task operation",
        },
    }
}

fn json_store_error(_error: serde_json::Error) -> StoreError {
    StoreError::AdministrationFailed {
        operation: "media state JSON conversion",
    }
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

pub fn data_labels(owner: &TaskOwner) -> Result<BTreeSet<DataLabelId>, StoreError> {
    owner
        .data_labels
        .iter()
        .map(|label| {
            DataLabelId::parse(label.clone()).map_err(|_| StoreError::InvalidIdentityField {
                field: "data_labels",
                reason: "invalid media task data label",
            })
        })
        .collect()
}
