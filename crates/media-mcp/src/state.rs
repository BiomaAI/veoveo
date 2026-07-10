//! Durable media task state backed by the installation SurrealDB.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use veoveo_mcp_contract::{
    ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret, DataLabelId,
    IssuedArtifactWriteCapability, UsageKind, UsageRecord,
};
use veoveo_platform_store::{
    ArtifactWriteCapabilityId as StoreCapabilityId, MediaTaskContextId, MediaTaskContextRecord,
    MediaUsageId, MediaUsageKind, MediaUsageRecord, OpenObject, OutboxDraft, PlatformStore,
    ProviderEventId, ProviderEventRecord, ProviderJobId, ProviderJobRecord, ProviderJobState,
    RecordId, RecordIdKey, RedactedSecret, StoreError, TaskId, TaskStatus,
};
use veoveo_task_runtime::{RecoveryClass, TaskFailure, TaskOwner, TaskRuntime, TaskSnapshot};

use crate::provider::Prediction;

const PROVIDER: &str = "media";
const TASK_EVENT_SCHEMA_VERSION: i64 = 2;
const MEDIA_EVENT_SCHEMA_VERSION: i64 = 1;
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
    pub external_job_id: String,
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

    fn event_type(&self) -> &'static str {
        match self {
            Self::Requested => "provider_job.cancel_requested",
            Self::Accepted { .. } => "provider_job.cancel_accepted",
            Self::NotDeleted { .. } => "provider_job.cancel_not_deleted",
            Self::Failed { .. } => "provider_job.cancel_failed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TaskRequestEnvelope {
    input: Value,
    owner: TaskOwner,
    status_message: Option<String>,
    ttl_ms: Option<u64>,
    poll_interval_ms: Option<u64>,
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
            task: task_id.record_id(),
            tenant: tenant_record(owner)?,
            capability: StoreCapabilityId::from_uuid(capability.capability_id.as_uuid())
                .record_id(),
            capability_secret: RedactedSecret::new(capability.secret.expose_secret()),
            capability_expires_at: capability.expires_at,
            created_at: now,
            updated_at: now,
        };
        let outbox = OutboxDraft::now(
            Some(tenant_record(owner)?),
            "media_task_context",
            task_id.to_string(),
            "media.task_context.created",
            MEDIA_EVENT_SCHEMA_VERSION,
            OpenObject::new(BTreeMap::from([
                ("task_id".into(), serde_json::json!(task_id.to_string())),
                (
                    "capability_id".into(),
                    serde_json::json!(capability.capability_id.to_string()),
                ),
            ])),
        );
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $context CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("context", context_id.record_id()))
            .bind(("content", content))
            .bind(("outbox", outbox))
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
            .query("SELECT * FROM ONLY $context;")
            .bind(("context", context_id.record_id()))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }

    pub async fn bind_submission_and_wait(
        &self,
        runtime: &TaskRuntime,
        task_id: &str,
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
                    key: prediction.id.clone(),
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
            task: current.task_id.record_id(),
            provider: PROVIDER.to_owned(),
            external_job_id: prediction.id.clone(),
            state: ProviderJobState::Waiting,
            provider_payload: prediction_payload(prediction)?,
            submitted_at: now,
            updated_at: now,
            completed_at: None,
        };
        let waiting = waiting_snapshot(
            &current,
            format!(
                "submitted; prediction {}; resource media://prediction/{}; waiting for signed provider webhook",
                prediction.id, prediction.id
            ),
            now,
        );
        let task_outbox = task_event(&waiting, "task.waiting")?;
        let job_outbox = provider_job_event(&current, &job, "provider_job.bound")?;
        let request = request_envelope(&waiting)?;
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $job CONTENT $job_content RETURN NONE; LET $updated = (UPDATE ONLY $task SET status = 'waiting', request = $request, progress = $progress, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now WHERE status = $expected_status AND updated_at = $expected_updated AND recovery_class = 'webhook_wait' AND lease_owner = $worker RETURN AFTER); IF $updated = NONE { THROW 'media task changed before provider binding'; }; CREATE outbox_event CONTENT $job_outbox RETURN NONE; CREATE outbox_event CONTENT $task_outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("job", job_id.record_id()))
            .bind(("job_content", job))
            .bind(("task", current.task_id.record_id()))
            .bind(("request", request))
            .bind(("progress", waiting.progress))
            .bind(("now", now))
            .bind(("expected_status", current.status))
            .bind(("expected_updated", current.updated_at))
            .bind(("worker", runtime.worker_id().to_owned()))
            .bind(("job_outbox", job_outbox))
            .bind(("task_outbox", task_outbox))
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
        task_id: &str,
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
                key: prediction.id.clone(),
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
            task: current.task_id.record_id(),
            provider: PROVIDER.to_owned(),
            external_job_id: prediction.id.clone(),
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
        let event_outbox = provider_event_outbox(&current, &event, prediction)?;
        let job_outbox = provider_job_event(&current, &job, "provider_job.webhook_received")?;
        let task_outbox = waiting
            .as_ref()
            .map(|snapshot| task_event(snapshot, "task.waiting"))
            .transpose()?;
        let request = waiting.as_ref().map(request_envelope).transpose()?;
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $event CONTENT $event_content RETURN NONE; UPSERT ONLY $job CONTENT $job_content RETURN NONE; IF $update_task { LET $updated = (UPDATE ONLY $task SET status = 'waiting', request = $request, progress = $progress, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now WHERE updated_at = $expected_updated AND status IN ['queued', 'running', 'waiting'] AND recovery_class = 'webhook_wait' RETURN AFTER); IF $updated = NONE { THROW 'media webhook task changed'; }; CREATE outbox_event CONTENT $task_outbox RETURN NONE; }; CREATE outbox_event CONTENT $job_outbox RETURN NONE; CREATE outbox_event CONTENT $event_outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("event", event_id.record_id()))
            .bind(("event_content", event))
            .bind(("job", job_id.record_id()))
            .bind(("job_content", job))
            .bind(("update_task", waiting.is_some()))
            .bind(("task", current.task_id.record_id()))
            .bind(("request", request))
            .bind(("progress", waiting.as_ref().map_or(current.progress, |task| task.progress)))
            .bind(("now", now))
            .bind(("expected_updated", current.updated_at))
            .bind(("task_outbox", task_outbox))
            .bind(("job_outbox", job_outbox))
            .bind(("event_outbox", event_outbox))
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
            .query("SELECT * FROM provider_event WHERE provider = $provider AND processed_at = NONE ORDER BY received_at ASC LIMIT $limit;")
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
            .get(&event.job.task_id.to_string())
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
            Ok(result) => (TaskStatus::Succeeded, Some(open_object(result)), None, 1.0),
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
        completed.result = result.clone().map(open_value);
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
        let request = request_envelope(&completed)?;
        let task_outbox = task_event(
            &completed,
            if status == TaskStatus::Succeeded {
                "task.succeeded"
            } else {
                "task.failed"
            },
        )?;
        let processed_outbox = OutboxDraft::now(
            Some(tenant_record(&current.owner)?),
            "provider_event",
            event.event_id.to_string(),
            "provider_event.processed",
            MEDIA_EVENT_SCHEMA_VERSION,
            OpenObject::new(BTreeMap::from([
                (
                    "task_id".into(),
                    serde_json::json!(current.task_id.to_string()),
                ),
                (
                    "external_job_id".into(),
                    serde_json::json!(&event.job.external_job_id),
                ),
            ])),
        );
        let response = self
            .store
            .client()
            .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $status, request = $request, progress = $progress, result = $result, error = $error, completed_at = $now, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now WHERE updated_at = $expected_updated AND status IN ['queued', 'running', 'waiting'] AND recovery_class = 'webhook_wait' RETURN AFTER); IF $updated = NONE { THROW 'media webhook completion conflict'; }; UPDATE ONLY $event SET processed_at = $now, processing_error = NONE WHERE processed_at = NONE RETURN NONE; UPDATE ONLY $job SET state = $job_state, completed_at = $now, updated_at = $now RETURN NONE; CREATE outbox_event CONTENT $task_outbox RETURN NONE; CREATE outbox_event CONTENT $processed_outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("task", current.task_id.record_id()))
            .bind(("status", status))
            .bind(("request", request))
            .bind(("progress", progress))
            .bind(("result", result))
            .bind(("error", error))
            .bind(("now", now))
            .bind(("expected_updated", current.updated_at))
            .bind(("event", event.event_id.record_id()))
            .bind(("job", event.job.job_id.record_id()))
            .bind(("job_state", if status == TaskStatus::Succeeded { ProviderJobState::Succeeded } else { ProviderJobState::Failed }))
            .bind(("task_outbox", task_outbox))
            .bind(("processed_outbox", processed_outbox))
            .await
            .and_then(|response| response.check());
        if let Err(error) = response {
            let latest = runtime
                .get(&current.task_id.to_string())
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
            .get(&current.task_id.to_string())
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
            .get(&event.job.task_id.to_string())
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
            .query("UPDATE ONLY $event SET processing_error = $error WHERE processed_at = NONE RETURN NONE;")
            .bind(("event", event.event_id.record_id()))
            .bind(("error", truncate(error, 2_000)))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn provider_job_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        let task_id = task_id
            .parse::<TaskId>()
            .map_err(|_| StoreError::MissingRecord {
                operation: "media provider task id",
            })?;
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM provider_job WHERE provider = $provider AND task = $task ORDER BY submitted_at ASC LIMIT 1;")
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("task", task_id.record_id()))
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
        let outbox = provider_cancellation_event(task, job, &outcome, state)?;
        self.store
            .client()
            .query("BEGIN TRANSACTION; UPDATE ONLY $job SET state = $state, provider_payload = $payload, completed_at = IF $terminal { $now } ELSE { completed_at }, updated_at = $now WHERE tenant = $tenant AND task = $task AND provider = $provider AND state IN ['submitted', 'waiting', 'cancel_requested'] RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("job", job.job_id.record_id()))
            .bind(("state", state))
            .bind(("payload", OpenObject::new(payload)))
            .bind(("terminal", state == ProviderJobState::Cancelled))
            .bind(("now", now))
            .bind(("tenant", tenant_record(&task.owner)?))
            .bind(("task", task.task_id.record_id()))
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.provider_job(job.job_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "media provider cancellation readback",
            })
    }

    pub async fn provider_job_for_external(
        &self,
        external_job_id: &str,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM provider_job WHERE provider = $provider AND external_job_id = $external_job_id ORDER BY submitted_at ASC LIMIT 1;")
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("external_job_id", external_job_id.to_owned()))
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
        external_job_id: &str,
        tenant: &RecordId,
    ) -> Result<Option<MediaProviderJob>, StoreError> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM provider_job WHERE tenant = $tenant AND provider = $provider AND external_job_id = $external_job_id LIMIT 1;")
            .bind(("tenant", tenant.clone()))
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("external_job_id", external_job_id.to_owned()))
            .await?
            .check()?;
        response
            .take::<Vec<ProviderJobRecord>>(0)?
            .into_iter()
            .next()
            .map(provider_job)
            .transpose()
    }

    pub async fn provider_jobs(&self) -> Result<Vec<MediaProviderJob>, StoreError> {
        let mut response = self
            .store
            .client()
            .query(
                "SELECT * FROM provider_job WHERE provider = $provider ORDER BY submitted_at ASC;",
            )
            .bind(("provider", PROVIDER.to_owned()))
            .await?
            .check()?;
        response
            .take::<Vec<ProviderJobRecord>>(0)?
            .into_iter()
            .map(provider_job)
            .collect()
    }

    pub async fn usage_records(&self, task_id: &str) -> Result<Vec<UsageRecord>, StoreError> {
        let task_id = task_id
            .parse::<TaskId>()
            .map_err(|_| StoreError::MissingRecord {
                operation: "media usage task id",
            })?;
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM media_usage WHERE task = $task ORDER BY recorded_at ASC; SELECT * FROM provider_job WHERE task = $task;")
            .bind(("task", task_id.record_id()))
            .await?
            .check()?;
        let records = response.take::<Vec<MediaUsageRecord>>(0)?;
        let mut provider_jobs = BTreeMap::new();
        for job in response.take::<Vec<ProviderJobRecord>>(1)? {
            provider_jobs.insert(record_uuid(&job.id)?, job.external_job_id);
        }
        records
            .into_iter()
            .map(|record| usage_record(record, &provider_jobs))
            .collect()
    }

    pub async fn usage_task_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut response = self
            .store
            .client()
            .query("SELECT VALUE task FROM media_usage GROUP BY task ORDER BY task ASC;")
            .await?
            .check()?;
        response
            .take::<Vec<RecordId>>(0)?
            .into_iter()
            .map(|record| record_uuid(&record).map(|id| id.to_string()))
            .collect()
    }

    pub async fn record_usage(
        &self,
        task: &TaskSnapshot,
        provider_job: Option<&MediaProviderJob>,
        usage: &UsageRecord,
    ) -> Result<(), StoreError> {
        let kind = match usage.kind {
            UsageKind::Estimate => MediaUsageKind::Estimate,
            UsageKind::Actual => MediaUsageKind::Actual,
        };
        let key = format!(
            "{}:{}:{}",
            task.task_id,
            match kind {
                MediaUsageKind::Estimate => "estimate",
                MediaUsageKind::Actual => "actual",
            },
            usage.source_id.as_deref().unwrap_or("initial")
        );
        let id = MediaUsageId::from_uuid(Uuid::new_v5(&STATE_ID_NAMESPACE, key.as_bytes()));
        let record = MediaUsageRecord {
            id: id.record_id(),
            tenant: tenant_record(&task.owner)?,
            task: task.task_id.record_id(),
            provider_job: provider_job.map(|job| job.job_id.record_id()),
            source_id: usage.source_id.clone(),
            model_id: usage.model_id.clone(),
            kind,
            quantity: usage.quantity,
            unit: usage.unit.clone(),
            amount: usage.amount,
            currency: usage.currency.clone(),
            metadata: open_object(usage.metadata.clone()),
            recorded_at: usage.recorded_at,
        };
        let outbox = OutboxDraft::now(
            Some(tenant_record(&task.owner)?),
            "media_usage",
            id.to_string(),
            "media.usage.recorded",
            MEDIA_EVENT_SCHEMA_VERSION,
            OpenObject::new(BTreeMap::from([(
                "task_id".into(),
                serde_json::json!(task.task_id.to_string()),
            )])),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; UPSERT ONLY $usage CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("usage", id.record_id()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn has_actual_usage(
        &self,
        task_id: &str,
        external_job_id: &str,
    ) -> Result<bool, StoreError> {
        let task = task_id
            .parse::<TaskId>()
            .map_err(|_| StoreError::MissingRecord {
                operation: "media actual usage task id",
            })?;
        let job = self.provider_job_for_external(external_job_id).await?;
        let Some(job) = job else {
            return Ok(false);
        };
        let mut response = self
            .store
            .client()
            .query("RETURN count((SELECT VALUE id FROM media_usage WHERE task = $task AND provider_job = $job AND kind = 'actual' LIMIT 1)) > 0;")
            .bind(("task", task.record_id()))
            .bind(("job", job.job_id.record_id()))
            .await?
            .check()?;
        Ok(response.take::<Option<bool>>(0)?.unwrap_or(false))
    }

    pub async fn delete_usage_before(&self, cutoff: DateTime<Utc>) -> Result<u64, StoreError> {
        let mut response = self
            .store
            .client()
            .query("DELETE media_usage WHERE recorded_at < $cutoff RETURN BEFORE;")
            .bind(("cutoff", cutoff))
            .await?
            .check()?;
        Ok(response.take::<Vec<MediaUsageRecord>>(0)?.len() as u64)
    }

    pub async fn prune_task_contexts(&self) -> Result<u64, StoreError> {
        let mut response = self
            .store
            .client()
            .query("DELETE media_task_context WHERE capability_expires_at <= time::now() RETURN BEFORE;")
            .await?
            .check()?;
        Ok(response.take::<Vec<MediaTaskContextRecord>>(0)?.len() as u64)
    }

    async fn ensure_task_waiting(
        &self,
        runtime: &TaskRuntime,
        task_id: &str,
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
                "submitted; prediction {}; resource media://prediction/{}; waiting for signed provider webhook",
                job.external_job_id, job.external_job_id
            ),
            now,
        );
        let outbox = task_event(&waiting, "task.waiting")?;
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = 'waiting', request = $request, progress = $progress, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now WHERE updated_at = $expected_updated AND status IN ['queued', 'running'] AND recovery_class = 'webhook_wait' RETURN AFTER); IF $updated != NONE { CREATE outbox_event CONTENT $outbox RETURN NONE; }; COMMIT TRANSACTION;")
            .bind(("task", current.task_id.record_id()))
            .bind(("request", request_envelope(&waiting)?))
            .bind(("progress", waiting.progress))
            .bind(("now", now))
            .bind(("expected_updated", current.updated_at))
            .bind(("outbox", outbox))
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
            .query("UPDATE ONLY $event SET processed_at = time::now(), processing_error = $error WHERE processed_at = NONE RETURN NONE;")
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
            .query("SELECT * FROM provider_event WHERE tenant = $tenant AND provider = $provider AND event_id = $event_id LIMIT 1;")
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
            .query("SELECT * FROM ONLY $job;")
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
        external_job_id: record.external_job_id,
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

fn request_envelope(snapshot: &TaskSnapshot) -> Result<OpenObject, StoreError> {
    let envelope = TaskRequestEnvelope {
        input: snapshot.request.clone(),
        owner: snapshot.owner.clone(),
        status_message: snapshot.status_message.clone(),
        ttl_ms: snapshot.ttl_ms,
        poll_interval_ms: snapshot.poll_interval_ms,
    };
    serde_json::to_value(envelope)
        .map(open_object)
        .map_err(json_store_error)
}

fn task_event(snapshot: &TaskSnapshot, event_type: &str) -> Result<OutboxDraft, StoreError> {
    Ok(OutboxDraft::now(
        Some(tenant_record(&snapshot.owner)?),
        "task",
        snapshot.task_id.to_string(),
        event_type,
        TASK_EVENT_SCHEMA_VERSION,
        OpenObject::new(BTreeMap::from([(
            "snapshot".into(),
            serde_json::to_value(snapshot).map_err(json_store_error)?,
        )])),
    ))
}

fn provider_job_event(
    task: &TaskSnapshot,
    job: &ProviderJobRecord,
    event_type: &str,
) -> Result<OutboxDraft, StoreError> {
    Ok(OutboxDraft::now(
        Some(tenant_record(&task.owner)?),
        "provider_job",
        record_uuid(&job.id)?.to_string(),
        event_type,
        MEDIA_EVENT_SCHEMA_VERSION,
        OpenObject::new(BTreeMap::from([
            (
                "task_id".into(),
                serde_json::json!(task.task_id.to_string()),
            ),
            (
                "external_job_id".into(),
                serde_json::json!(&job.external_job_id),
            ),
            (
                "state".into(),
                serde_json::to_value(job.state).map_err(json_store_error)?,
            ),
        ])),
    ))
}

fn provider_cancellation_event(
    task: &TaskSnapshot,
    job: &MediaProviderJob,
    outcome: &ProviderCancellationOutcome,
    state: ProviderJobState,
) -> Result<OutboxDraft, StoreError> {
    Ok(OutboxDraft::now(
        Some(tenant_record(&task.owner)?),
        "provider_job",
        job.job_id.to_string(),
        outcome.event_type(),
        MEDIA_EVENT_SCHEMA_VERSION,
        OpenObject::new(BTreeMap::from([
            (
                "task_id".into(),
                serde_json::json!(task.task_id.to_string()),
            ),
            (
                "external_job_id".into(),
                serde_json::json!(&job.external_job_id),
            ),
            (
                "state".into(),
                serde_json::to_value(state).map_err(json_store_error)?,
            ),
            (
                "cancellation".into(),
                serde_json::to_value(outcome).map_err(json_store_error)?,
            ),
        ])),
    ))
}

fn provider_event_outbox(
    task: &TaskSnapshot,
    event: &ProviderEventRecord,
    prediction: &Prediction,
) -> Result<OutboxDraft, StoreError> {
    Ok(OutboxDraft::now(
        Some(tenant_record(&task.owner)?),
        "provider_event",
        record_uuid(&event.id)?.to_string(),
        "provider_event.received",
        MEDIA_EVENT_SCHEMA_VERSION,
        OpenObject::new(BTreeMap::from([
            (
                "task_id".into(),
                serde_json::json!(task.task_id.to_string()),
            ),
            ("external_job_id".into(), serde_json::json!(&prediction.id)),
            ("webhook_id".into(), serde_json::json!(&event.event_id)),
        ])),
    ))
}

fn tenant_record(owner: &TaskOwner) -> Result<RecordId, StoreError> {
    veoveo_platform_store::deterministic_tenant_id(owner.tenant_key()).map(|id| id.record_id())
}

fn open_object(value: Value) -> OpenObject {
    match value {
        Value::Object(values) => OpenObject::new(values.into_iter().collect()),
        value => OpenObject::new(BTreeMap::from([("value".into(), value)])),
    }
}

fn open_value(value: OpenObject) -> Value {
    let mut values: serde_json::Map<String, Value> = value.into_map().into_iter().collect();
    if values.len() == 1 && values.contains_key("value") {
        values.remove("value").unwrap_or(Value::Null)
    } else {
        Value::Object(values)
    }
}

fn record_uuid(record: &RecordId) -> Result<Uuid, StoreError> {
    match &record.key {
        RecordIdKey::Uuid(value) => Ok(**value),
        _ => Err(StoreError::MissingRecord {
            operation: "media UUID record decoding",
        }),
    }
}

fn usage_record(
    record: MediaUsageRecord,
    provider_jobs: &BTreeMap<Uuid, String>,
) -> Result<UsageRecord, StoreError> {
    let provider_job_id = match record.provider_job.as_ref() {
        Some(job) => provider_jobs.get(&record_uuid(job)?).cloned(),
        None => None,
    };
    Ok(UsageRecord {
        task_id: record_uuid(&record.task)?.to_string(),
        source_id: record.source_id,
        provider_job_id,
        model_id: record.model_id,
        kind: match record.kind {
            MediaUsageKind::Estimate => UsageKind::Estimate,
            MediaUsageKind::Actual => UsageKind::Actual,
        },
        quantity: record.quantity,
        unit: record.unit,
        amount: record.amount,
        currency: record.currency,
        recorded_at: record.recorded_at,
        metadata: open_value(record.metadata),
    })
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
            DataLabelId::new(label.clone()).map_err(|_| StoreError::InvalidIdentityField {
                field: "data_labels",
                reason: "invalid media task data label",
            })
        })
        .collect()
}
