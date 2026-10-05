//! Durable media task state backed by the installation SurrealDB.

mod usage;
pub use usage::MediaBillingPage;

use std::collections::BTreeSet;
use veoveo_platform_store::task_record_id;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;
use veoveo_mcp_contract::{
    ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret, IssuedArtifactWriteCapability,
};
use veoveo_platform_store::{
    ArtifactWriteCapabilityId as StoreCapabilityId, MediaTaskContextId, MediaTaskContextRecord,
    OpenObject, PlatformStore, ProviderEventId, ProviderEventRecord, ProviderJobId,
    ProviderJobRecord, ProviderJobState, RecordId, RecordIdKey, RedactedSecret, StoreError,
    TaskStatus,
};
use veoveo_task_runtime::{RecoveryClass, TaskFailure, TaskOwner, TaskRuntime, TaskSnapshot};
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
}

#[derive(Clone, Debug)]
pub struct WebhookReceipt {
    pub event: MediaProviderEvent,
    pub inserted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ProviderCancellationOutcome {
    Requested,
    Accepted { deleted_count: u64 },
    NotDeleted { deleted_count: u64 },
    Failed { error: String },
}

impl ProviderCancellationOutcome {
    fn job_state(&self) -> ProviderJobState {
        match self {
            Self::Accepted { .. } => ProviderJobState::Cancelled,
            Self::Requested | Self::NotDeleted { .. } | Self::Failed { .. } => {
                ProviderJobState::CancelRequested
            }
        }
    }
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

    pub async fn bind_submission_and_wait(
        &self,
        runtime: &TaskRuntime,
        task_id: TaskId,
        prediction: &Prediction,
    ) -> Result<MediaProviderJob, StoreError> {
        let current = runtime
            .get(task_id)
            .await
            .map_err(task_store_error)?
            .ok_or(StoreError::MissingRecord {
                operation: "media provider submission task",
            })?;
        validate_webhook_task(&current)?;
        let tenant = tenant_record(&current.owner)?;
        if let Some(job) = self
            .provider_job_for_external_in_tenant(&prediction.id, &tenant)
            .await?
        {
            if job.task_id != current.task_id {
                return Err(StoreError::ArtifactWriteConflict {
                    key: prediction.id.to_string(),
                });
            }
            self.ensure_task_waiting(runtime, task_id, &job).await?;
            return Ok(job);
        }
        let job_id = ProviderJobId::new();
        let now = Utc::now();
        let job = ProviderJobRecord {
            id: job_id.record_id(),
            tenant: tenant.clone(),
            task: task_record_id(current.task_id),
            provider: PROVIDER.to_owned(),
            external_job_id: prediction.id.to_string(),
            state: ProviderJobState::Waiting,
            provider_payload: prediction_payload(prediction)?,
            submitted_at: now,
            updated_at: now,
            completed_at: None,
        };
        let waiting = waiting_snapshot(
            &current,
            format!(
                "submitted; prediction {}; resource {}; waiting for signed provider webhook",
                prediction.id,
                MediaPredictionUri::new(prediction.id.clone())
            ),
            now,
        );

        let request = veoveo_platform_store::TaskRequestRecord::from(&waiting);
        let result = self
            .store
            .client()
            .query(include_str!("queries/bind_provider_job.surql"))
            .bind(("job", job_id.record_id()))
            .bind(("job_content", job))
            .bind(("task", task_record_id(current.task_id)))
            .bind(("request", request))
            .bind(("progress", waiting.progress))
            .bind(("now", now))
            .bind(("expected_status", current.status))
            .bind(("expected_updated", current.updated_at))
            .bind((
                "expected_request",
                veoveo_platform_store::TaskRequestRecord::from(&current),
            ))
            .bind((
                "expected_owner_context",
                veoveo_platform_store::TaskOwnerRecord::try_from(&current.owner)
                    .map_err(task_store_error)?,
            ))
            .bind(("worker", runtime.worker_id().to_owned()))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .provider_job_for_external_in_tenant(&prediction.id, &tenant)
                .await?
                && existing.task_id == current.task_id
            {
                return Ok(existing);
            }
            return Err(error.into());
        }
        self.provider_job_for_external_in_tenant(&prediction.id, &tenant)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "media provider job readback",
            })
    }

    pub async fn receive_webhook(
        &self,
        runtime: &TaskRuntime,
        task_id: TaskId,
        webhook_id: &str,
        prediction: &Prediction,
    ) -> Result<WebhookReceipt, StoreError> {
        if !prediction.is_terminal() {
            return Err(StoreError::ArtifactWriteConflict {
                key: webhook_id.to_owned(),
            });
        }
        let current = runtime
            .get(task_id)
            .await
            .map_err(task_store_error)?
            .ok_or(StoreError::MissingRecord {
                operation: "media webhook task",
            })?;
        validate_webhook_task(&current)?;
        let tenant = tenant_record(&current.owner)?;
        if let Some(event) = self.provider_event(&tenant, webhook_id).await? {
            validate_webhook_replay(&event, &current, prediction)?;
            return Ok(WebhookReceipt {
                event,
                inserted: false,
            });
        }
        let existing_job = self
            .provider_job_for_external_in_tenant(&prediction.id, &tenant)
            .await?;
        if existing_job
            .as_ref()
            .is_some_and(|job| job.task_id != current.task_id)
        {
            return Err(StoreError::ArtifactWriteConflict {
                key: prediction.id.to_string(),
            });
        }
        let job_id = existing_job
            .as_ref()
            .map_or_else(ProviderJobId::new, |job| job.job_id);
        let event_id = provider_event_id(&current.owner, webhook_id);
        let now = Utc::now();
        // A cancellation acknowledgement is only best effort. A later signed
        // provider webhook remains authoritative for the provider job's actual
        // terminal state, while the locally cancelled task stays immutable.
        let preserve_terminal_job = existing_job.as_ref().is_some_and(|job| {
            matches!(
                job.state,
                ProviderJobState::Succeeded | ProviderJobState::Failed
            )
        });
        let job_prediction = existing_job
            .as_ref()
            .filter(|_| preserve_terminal_job)
            .map_or(prediction, |job| &job.prediction);
        let job_state = existing_job
            .as_ref()
            .filter(|_| preserve_terminal_job)
            .map_or_else(|| prediction_state(prediction), |job| job.state);
        let job = ProviderJobRecord {
            id: job_id.record_id(),
            tenant: tenant.clone(),
            task: task_record_id(current.task_id),
            provider: PROVIDER.to_owned(),
            external_job_id: prediction.id.to_string(),
            state: job_state,
            provider_payload: prediction_payload(job_prediction)?,
            submitted_at: existing_job
                .as_ref()
                .map_or(current.created_at, |job| job.updated_at),
            updated_at: now,
            completed_at: Some(now),
        };
        let event = ProviderEventRecord {
            id: event_id.record_id(),
            tenant: tenant.clone(),
            provider_job: job_id.record_id(),
            provider: PROVIDER.to_owned(),
            event_id: webhook_id.to_owned(),
            signing_key_id: None,
            payload: prediction_payload(prediction)?,
            received_at: now,
            processed_at: None,
            processing_error: None,
        };
        let waiting = (!current.is_terminal() && current.status != TaskStatus::CancelRequested)
            .then(|| {
                waiting_snapshot(
                    &current,
                    format!("signed webhook received for prediction {}", prediction.id),
                    now,
                )
            });

        let request = waiting
            .as_ref()
            .map(veoveo_platform_store::TaskRequestRecord::from);
        let result = self
            .store
            .client()
            .query(include_str!("queries/record_provider_event.surql"))
            .bind(("event", event_id.record_id()))
            .bind(("event_content", event))
            .bind(("job", job_id.record_id()))
            .bind(("job_content", job))
            .bind(("update_task", waiting.is_some()))
            .bind(("task", task_record_id(current.task_id)))
            .bind(("request", request))
            .bind((
                "progress",
                waiting
                    .as_ref()
                    .map_or(current.progress, |task| task.progress),
            ))
            .bind(("now", now))
            .bind(("expected_updated", current.updated_at))
            .bind((
                "expected_request",
                veoveo_platform_store::TaskRequestRecord::from(&current),
            ))
            .bind((
                "expected_owner_context",
                veoveo_platform_store::TaskOwnerRecord::try_from(&current.owner)
                    .map_err(task_store_error)?,
            ))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(event) = self.provider_event(&tenant, webhook_id).await? {
                validate_webhook_replay(&event, &current, prediction)?;
                return Ok(WebhookReceipt {
                    event,
                    inserted: false,
                });
            }
            return Err(error.into());
        }
        Ok(WebhookReceipt {
            event: self.provider_event(&tenant, webhook_id).await?.ok_or(
                StoreError::MissingRecord {
                    operation: "media webhook event readback",
                },
            )?,
            inserted: true,
        })
    }

    pub async fn pending_events(
        &self,
        limit: usize,
    ) -> Result<Vec<MediaProviderEvent>, StoreError> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/pending_provider_events.surql"))
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("limit", i64::try_from(limit).unwrap_or(i64::MAX)))
            .await?
            .check()?;
        let events: Vec<ProviderEventRecord> = response.take(0)?;
        let mut result = Vec::with_capacity(events.len());
        for event in events {
            result.push(self.map_event(event).await?);
        }
        Ok(result)
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
                operation: "media webhook completion task",
            })?;
        if current.is_terminal() {
            self.acknowledge_event(event, None).await?;
            return Ok(current);
        }
        validate_webhook_task(&current)?;
        let now = Utc::now();
        let (status, result, error, progress) = match result {
            Ok(result) => (
                TaskStatus::Succeeded,
                Some(veoveo_platform_store::TaskResultRecord::new(result)),
                None,
                1.0,
            ),
            Err(error) => (
                TaskStatus::Failed,
                None,
                Some(open_object(
                    serde_json::to_value(error).map_err(json_store_error)?,
                )),
                current.progress,
            ),
        };
        let mut completed = current.clone();
        completed.status = status;
        completed.status_message = Some(message.clone());
        completed.progress = progress;
        completed.result = result
            .clone()
            .map(veoveo_platform_store::TaskResultRecord::into_payload);
        completed.error = error
            .clone()
            .map(open_value)
            .map(serde_json::from_value)
            .transpose()
            .map_err(json_store_error)?;
        completed.completed_at = Some(now);
        completed.updated_at = now;
        completed.lease_owner = None;
        completed.lease_expires_at = None;
        let request = veoveo_platform_store::TaskRequestRecord::from(&completed);

        let response = self
            .store
            .client()
            .query(include_str!("queries/settle_provider_event.surql"))
            .bind(("task", task_record_id(current.task_id)))
            .bind(("status", status))
            .bind(("request", request))
            .bind(("progress", progress))
            .bind(("result", result))
            .bind(("error", error))
            .bind(("now", now))
            .bind(("expected_updated", current.updated_at))
            .bind((
                "expected_request",
                veoveo_platform_store::TaskRequestRecord::from(&current),
            ))
            .bind((
                "expected_owner_context",
                veoveo_platform_store::TaskOwnerRecord::try_from(&current.owner)
                    .map_err(task_store_error)?,
            ))
            .bind(("event", event.event_id.record_id()))
            .bind(("job", event.job.job_id.record_id()))
            .bind((
                "job_state",
                if status == TaskStatus::Succeeded {
                    ProviderJobState::Succeeded
                } else {
                    ProviderJobState::Failed
                },
            ))
            .await
            .and_then(|response| response.check());
        if let Err(error) = response {
            let latest = runtime
                .get(current.task_id)
                .await
                .map_err(task_store_error)?;
            if let Some(latest) = latest
                && latest.is_terminal()
            {
                self.acknowledge_event(event, None).await?;
                return Ok(latest);
            }
            return Err(error.into());
        }
        runtime
            .get(current.task_id)
            .await
            .map_err(task_store_error)?
            .ok_or(StoreError::MissingRecord {
                operation: "media completed task readback",
            })
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
                operation: "media cancelled webhook task",
            })?;
        if !matches!(
            current.status,
            TaskStatus::CancelRequested | TaskStatus::Cancelled
        ) {
            return Err(StoreError::ArtifactWriteConflict {
                key: current.task_id.to_string(),
            });
        }
        self.acknowledge_event(event, None).await?;
        Ok(current)
    }

    pub async fn record_processing_error(
        &self,
        event: &MediaProviderEvent,
        error: &str,
    ) -> Result<(), StoreError> {
        self.store
            .client()
            .query(include_str!("queries/record_event_error.surql"))
            .bind(("event", event.event_id.record_id()))
            .bind(("error", truncate(error, 2_000)))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn provider_job_for_task(
        &self,
        task_id: TaskId,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/task_provider_job.surql"))
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("task", task_record_id(task_id)))
            .await?
            .check()?;
        response
            .take::<Vec<ProviderJobRecord>>(0)?
            .into_iter()
            .next()
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
            return Err(StoreError::MissingRecord {
                operation: "media provider cancellation task binding",
            });
        }
        let now = Utc::now();
        let state = outcome.job_state();
        let mut prediction = job.prediction.clone();
        if state == ProviderJobState::Cancelled {
            prediction.status = "cancelled".to_owned();
        }
        let mut payload = prediction_payload(&prediction)?.into_map();
        payload.insert(
            "cancellation".to_owned(),
            serde_json::json!({
                "recorded_at": now,
                "result": &outcome,
            }),
        );

        self.store
            .client()
            .query(include_str!("queries/update_provider_job.surql"))
            .bind(("job", job.job_id.record_id()))
            .bind(("state", state))
            .bind(("payload", OpenObject::new(payload)))
            .bind(("terminal", state == ProviderJobState::Cancelled))
            .bind(("now", now))
            .bind(("tenant", tenant_record(&task.owner)?))
            .bind(("task", task_record_id(task.task_id)))
            .bind(("provider", PROVIDER.to_owned()))
            .await?
            .check()?;
        self.provider_job(job.job_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "media provider cancellation readback",
            })
    }

    pub async fn provider_job_for_task_prediction(
        &self,
        task_id: TaskId,
        prediction_id: &MediaPredictionId,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/linked_provider_job.surql"))
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("task", task_record_id(task_id)))
            .bind(("prediction", prediction_id.to_string()))
            .await?
            .check()?;
        response
            .take::<Vec<ProviderJobRecord>>(0)?
            .into_iter()
            .next()
            .map(provider_job)
            .transpose()
    }

    async fn provider_job_for_external_in_tenant(
        &self,
        external_job_id: &MediaPredictionId,
        tenant: &RecordId,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/prediction_provider_job.surql"))
            .bind(("tenant", tenant.clone()))
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("external_job_id", external_job_id.to_string()))
            .await?
            .check()?;
        response
            .take::<Vec<ProviderJobRecord>>(0)?
            .into_iter()
            .next()
            .map(provider_job)
            .transpose()
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

    async fn ensure_task_waiting(
        &self,
        runtime: &TaskRuntime,
        task_id: TaskId,
        job: &MediaProviderJob,
    ) -> Result<(), StoreError> {
        let Some(current) = runtime.get(task_id).await.map_err(task_store_error)? else {
            return Err(StoreError::MissingRecord {
                operation: "media waiting task",
            });
        };
        if current.status == TaskStatus::Waiting || current.is_terminal() {
            return Ok(());
        }
        let now = Utc::now();
        let waiting = waiting_snapshot(
            &current,
            format!(
                "submitted; prediction {}; resource {}; waiting for signed provider webhook",
                job.external_job_id,
                MediaPredictionUri::new(job.external_job_id.clone())
            ),
            now,
        );

        self.store
            .client()
            .query(include_str!("queries/wait_for_webhook.surql"))
            .bind(("task", task_record_id(current.task_id)))
            .bind((
                "request",
                veoveo_platform_store::TaskRequestRecord::from(&waiting),
            ))
            .bind(("progress", waiting.progress))
            .bind(("now", now))
            .bind(("expected_updated", current.updated_at))
            .bind((
                "expected_request",
                veoveo_platform_store::TaskRequestRecord::from(&current),
            ))
            .bind((
                "expected_owner_context",
                veoveo_platform_store::TaskOwnerRecord::try_from(&current.owner)
                    .map_err(task_store_error)?,
            ))
            .await?
            .check()?;
        Ok(())
    }

    async fn acknowledge_event(
        &self,
        event: &MediaProviderEvent,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        self.store
            .client()
            .query(include_str!("queries/reject_provider_event.surql"))
            .bind(("event", event.event_id.record_id()))
            .bind(("error", error.map(|value| truncate(value, 2_000))))
            .await?
            .check()?;
        Ok(())
    }

    async fn provider_event(
        &self,
        tenant: &RecordId,
        webhook_id: &str,
    ) -> Result<Option<MediaProviderEvent>, StoreError> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/provider_event_by_id.surql"))
            .bind(("tenant", tenant.clone()))
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("event_id", webhook_id.to_owned()))
            .await?
            .check()?;
        let event = response
            .take::<Vec<ProviderEventRecord>>(0)?
            .into_iter()
            .next();
        match event {
            Some(event) => self.map_event(event).await.map(Some),
            None => Ok(None),
        }
    }

    async fn map_event(
        &self,
        event: ProviderEventRecord,
    ) -> Result<MediaProviderEvent, StoreError> {
        let prediction = prediction_from_payload(event.payload.clone())?;
        let job_id = ProviderJobId::from_uuid(record_uuid(&event.provider_job)?);
        let job = self
            .provider_job(job_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "media provider event job",
            })?;
        Ok(MediaProviderEvent {
            event_id: ProviderEventId::from_uuid(record_uuid(&event.id)?),
            webhook_id: event.event_id,
            job,
            prediction,
            processed_at: event.processed_at,
        })
    }

    async fn provider_job(
        &self,
        job_id: ProviderJobId,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/read_provider_job.surql"))
            .bind(("job", job_id.record_id()))
            .await?
            .check()?;
        response
            .take::<Option<ProviderJobRecord>>(0)?
            .map(provider_job)
            .transpose()
    }
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

fn prediction_payload(prediction: &Prediction) -> Result<OpenObject, StoreError> {
    serde_json::to_value(prediction)
        .map(open_object)
        .map_err(json_store_error)
}

fn prediction_from_payload(payload: OpenObject) -> Result<Prediction, StoreError> {
    serde_json::from_value(open_value(payload)).map_err(json_store_error)
}

fn prediction_state(prediction: &Prediction) -> ProviderJobState {
    if prediction.status == "completed" {
        ProviderJobState::Succeeded
    } else {
        ProviderJobState::Failed
    }
}

fn provider_event_id(owner: &TaskOwner, webhook_id: &str) -> ProviderEventId {
    ProviderEventId::from_uuid(Uuid::new_v5(
        &STATE_ID_NAMESPACE,
        format!("{}:{PROVIDER}:{webhook_id}", owner.tenant_key()).as_bytes(),
    ))
}

fn waiting_snapshot(current: &TaskSnapshot, message: String, now: DateTime<Utc>) -> TaskSnapshot {
    let mut waiting = current.clone();
    waiting.status = TaskStatus::Waiting;
    waiting.status_message = Some(message);
    waiting.progress = waiting.progress.max(0.3);
    waiting.lease_owner = None;
    waiting.lease_expires_at = None;
    waiting.updated_at = now;
    waiting
}

fn validate_webhook_task(snapshot: &TaskSnapshot) -> Result<(), StoreError> {
    if snapshot.recovery_class != RecoveryClass::WebhookWait {
        return Err(StoreError::ArtifactWriteConflict {
            key: snapshot.task_id.to_string(),
        });
    }
    Ok(())
}

fn validate_webhook_replay(
    event: &MediaProviderEvent,
    task: &TaskSnapshot,
    prediction: &Prediction,
) -> Result<(), StoreError> {
    if event.job.task_id == task.task_id && event.job.external_job_id == prediction.id {
        Ok(())
    } else {
        Err(StoreError::ArtifactWriteConflict {
            key: event.webhook_id.clone(),
        })
    }
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

fn open_value(value: OpenObject) -> Value {
    Value::Object(value.into_map().into_iter().collect())
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
    StoreError::AdministrationFailed {
        operation: match error {
            veoveo_task_runtime::TaskError::NotFound(_) => "media task not found",
            _ => "media durable task operation",
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
