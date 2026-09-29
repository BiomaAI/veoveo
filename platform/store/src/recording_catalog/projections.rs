//! Recording projection admission and atomic receipt lifecycle.
use std::{collections::BTreeSet, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_types::{DataLabelId, PolicyVersion, Sha256Digest};

use super::validate_text;
use crate::{
    PlatformStore, PrincipalId, RecordingDatasetId, RecordingId, RecordingProjectionReceiptId,
    RecordingProjectionReceiptRecord, RecordingProjectionState, RecordingReadGrantClass,
    RecordingReadGrantId, StoreError, TenantId, WorkContextId, primary_transaction_error,
};

/// Supplied by the authenticated service after policy admission.
#[derive(Clone, Debug)]
pub struct RecordingProjectionScope {
    pub tenant_id: TenantId,
    pub actor_id: PrincipalId,
    pub work_context_id: WorkContextId,
    pub policy_revision: PolicyVersion,
    pub data_labels: BTreeSet<DataLabelId>,
}

/// A validated idempotency key and the immutable input it identifies.
#[derive(Clone, Debug)]
pub struct RecordingProjectionRequest {
    dataset_id: RecordingDatasetId,
    recording_id: RecordingId,
    caller_idempotency_key: String,
    manifest_digest: Sha256Digest,
    query_digest: Sha256Digest,
}

impl RecordingProjectionRequest {
    pub fn new(
        dataset_id: RecordingDatasetId,
        recording_id: RecordingId,
        caller_idempotency_key: impl Into<String>,
        manifest_digest: Sha256Digest,
        query_digest: Sha256Digest,
    ) -> Result<Self, StoreError> {
        let caller_idempotency_key = caller_idempotency_key.into();
        validate_text("projection idempotency key", &caller_idempotency_key, 128)?;
        Ok(Self {
            dataset_id,
            recording_id,
            caller_idempotency_key,
            manifest_digest,
            query_digest,
        })
    }

    fn bindings(&self) -> RequestBindings {
        RequestBindings {
            dataset: self.dataset_id.record_id(),
            key: self.caller_idempotency_key.clone(),
            manifest: self.manifest_digest.hex().to_owned(),
            query: self.query_digest.hex().to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordingProjectionReceiptDraft {
    pub scope: RecordingProjectionScope,
    pub request: RecordingProjectionRequest,
    pub grant_id: RecordingReadGrantId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, SurrealValue)]
struct ScopeBindings {
    tenant: RecordId,
    actor: RecordId,
    context: RecordId,
    policy: String,
    labels: Vec<String>,
}
impl RecordingProjectionScope {
    fn bindings(&self) -> ScopeBindings {
        ScopeBindings {
            tenant: self.tenant_id.record_id(),
            actor: self.actor_id.record_id(),
            context: self.work_context_id.record_id(),
            policy: self.policy_revision.to_string(),
            labels: self.data_labels.iter().map(ToString::to_string).collect(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, SurrealValue)]
struct RequestBindings {
    dataset: RecordId,
    key: String,
    manifest: String,
    query: String,
}

// Only repository-owned clauses enter SQL text. All caller values use driver bindings.
const ADMITTED: &str = "tenant = $scope.tenant AND actor = $scope.actor
    AND work_context = $scope.context AND policy_revision = $scope.policy
    AND recordings = [$recording] AND expires_at > time::now()
    AND $recording.tenant = $scope.tenant AND $scope.labels CONTAINSALL $recording.labels
    AND dataset = $recording.dataset AND dataset.tenant = $scope.tenant
    AND grant.tenant = $scope.tenant AND grant.actor = $scope.actor
    AND grant.work_context = $scope.context AND grant.policy_revision = $scope.policy
    AND grant.grant_class = $grant_class AND grant.dataset = dataset
    AND grant.recordings = recordings AND grant.catalog_revision = catalog_revision
    AND grant.expires_at >= expires_at AND grant.expires_at > time::now()";
const REQUEST_MATCH: &str = "dataset = $request.dataset AND manifest_digest = $request.manifest
    AND query_digest = $request.query";
const EXISTING_ID: &str = "(SELECT VALUE id FROM recording_projection_receipt
    WHERE tenant = $scope.tenant AND actor = $scope.actor
      AND caller_idempotency_key = $request.key LIMIT 1)[0]";
const GRANT_ADMITTED: &str = "tenant = $scope.tenant AND actor = $scope.actor
    AND work_context = $scope.context AND policy_revision = $scope.policy
    AND grant_class = $grant_class AND recordings = [$recording]
    AND dataset = $request.dataset AND dataset = $recording.dataset
    AND dataset.tenant = $scope.tenant AND $recording.tenant = $scope.tenant
    AND $scope.labels CONTAINSALL $recording.labels
    AND expires_at >= $expires AND $expires > time::now()";

impl PlatformStore {
    pub async fn ready_recording_projection(
        &self,
        scope: &RecordingProjectionScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, StoreError> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM ONLY $projection WHERE {ADMITTED} AND state = $ready;"
            ))
            .bind(("projection", projection_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .bind(("scope", scope.bindings()))
            .bind(("ready", RecordingProjectionState::Ready))
            .bind(("grant_class", RecordingReadGrantClass::AppProjection))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// An existing actor-owned key binds the original context, policy and input.
    /// A mismatched row produces a conflict inside SQL before any receipt is decoded.
    pub async fn recording_projection_by_idempotency_key(
        &self,
        scope: &RecordingProjectionScope,
        request: &RecordingProjectionRequest,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, StoreError> {
        let mut response = self
            .db
            .query(format!(
                "BEGIN TRANSACTION;
             LET $existing = {EXISTING_ID};
             LET $result = IF $existing != NONE {{
                 LET $row = (SELECT * FROM ONLY $existing WHERE {ADMITTED} AND {REQUEST_MATCH});
                 IF $row = NONE {{ THROW 'recording_projection_request_conflict'; }};
                 $row
             }} ELSE {{ NONE }};
             RETURN $result;
             COMMIT TRANSACTION;"
            ))
            .bind(("scope", scope.bindings()))
            .bind(("recording", request.recording_id.record_id()))
            .bind(("request", request.bindings()))
            .bind(("grant_class", RecordingReadGrantClass::AppProjection))
            .await?;
        if let Some(error) = primary_transaction_error(response.take_errors()) {
            return Err(request_error(error));
        }
        Ok(response.take(3)?)
    }

    pub async fn reserve_recording_projection(
        &self,
        draft: RecordingProjectionReceiptDraft,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        let id = RecordingProjectionReceiptId::new();
        let sql = format!(
            "BEGIN TRANSACTION;
             LET $existing = {EXISTING_ID};
             LET $result = IF $existing != NONE {{
                 LET $row = (SELECT * FROM ONLY $existing WHERE {ADMITTED} AND {REQUEST_MATCH});
                 IF $row = NONE {{ THROW 'recording_projection_request_conflict'; }};
                 $row
             }} ELSE {{
                 LET $admitted = (SELECT * FROM ONLY $grant WHERE {GRANT_ADMITTED});
                 IF $admitted = NONE {{ THROW 'recording_projection_grant_conflict'; }};
                 CREATE ONLY $projection SET
                     tenant = $scope.tenant, actor = $scope.actor,
                     work_context = $scope.context, policy_revision = $scope.policy,
                     grant = $grant, dataset = $request.dataset, recordings = [$recording],
                     catalog_revision = $admitted.catalog_revision,
                     caller_idempotency_key = $request.key,
                     manifest_digest = $request.manifest, query_digest = $request.query,
                     state = $reserved, expires_at = $expires,
                     created_at = time::now(), updated_at = time::now()
             }};
             RETURN $result;
             COMMIT TRANSACTION;"
        );
        for attempt in 0..8 {
            let mut response = self
                .db
                .query(sql.clone())
                .bind(("projection", id.record_id()))
                .bind(("scope", draft.scope.bindings()))
                .bind(("request", draft.request.bindings()))
                .bind(("recording", draft.request.recording_id.record_id()))
                .bind(("grant", draft.grant_id.record_id()))
                .bind(("expires", draft.expires_at))
                .bind(("reserved", RecordingProjectionState::Reserved))
                .bind(("grant_class", RecordingReadGrantClass::AppProjection))
                .await?;
            let Some(error) = primary_transaction_error(response.take_errors()) else {
                return response
                    .take::<Option<RecordingProjectionReceiptRecord>>(3)?
                    .ok_or(StoreError::MissingRecord {
                        operation: "recording projection reservation",
                    });
            };
            if retryable(&error) && attempt < 7 {
                retry_delay(attempt).await;
                continue;
            }
            if error.is_thrown()
                && error
                    .message()
                    .contains("recording_projection_grant_conflict")
            {
                return Err(StoreError::RecordingReadGrantConflict {
                    grant_id: draft.grant_id.to_string(),
                });
            }
            // A concurrent insertion may have won the unique key. Read the admitted
            // winner; a transport failure never triggers another mutation here.
            if error
                .message()
                .contains("recording_projection_idempotency_unique")
                && let Some(winner) = self
                    .recording_projection_by_idempotency_key(&draft.scope, &draft.request)
                    .await?
            {
                return Ok(winner);
            }
            return Err(request_error(error));
        }
        unreachable!("bounded transaction retry")
    }

    pub async fn begin_recording_projection(
        &self,
        scope: &RecordingProjectionScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        self.transition_projection(scope, recording_id, projection_id, Transition::Begin)
            .await
    }

    pub async fn complete_recording_projection(
        &self,
        scope: &RecordingProjectionScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
        result_byte_len: u64,
        result_sha256: &Sha256Digest,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        let byte_len =
            i64::try_from(result_byte_len).map_err(|_| StoreError::InvalidRecordingField {
                field: "projection result_byte_len",
                reason: "must fit a signed 64-bit byte count",
            })?;
        self.transition_projection(
            scope,
            recording_id,
            projection_id,
            Transition::Complete {
                byte_len,
                digest: result_sha256,
            },
        )
        .await
    }

    pub async fn fail_recording_projection(
        &self,
        scope: &RecordingProjectionScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        self.transition_projection(
            scope,
            recording_id,
            projection_id,
            Transition::Failed(reason),
        )
        .await
    }

    pub async fn cancel_recording_projection(
        &self,
        scope: &RecordingProjectionScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        self.transition_projection(
            scope,
            recording_id,
            projection_id,
            Transition::Cancelled(reason),
        )
        .await
    }

    async fn transition_projection(
        &self,
        scope: &RecordingProjectionScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
        transition: Transition<'_>,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        let (target, predecessors, repeat_matches, assignment) = match transition {
            Transition::Begin => (
                RecordingProjectionState::Materializing,
                vec![RecordingProjectionState::Reserved],
                "true",
                "state = $target",
            ),
            Transition::Complete { .. } => (
                RecordingProjectionState::Ready,
                vec![RecordingProjectionState::Materializing],
                "result_byte_len = $bytes AND result_sha256 = $digest",
                "state = $target, result_byte_len = $bytes, result_sha256 = $digest, failure_reason = NONE",
            ),
            Transition::Failed(_) | Transition::Cancelled(_) => {
                validate_text(
                    "projection failure_reason",
                    transition.reason().expect("terminal transition"),
                    2048,
                )?;
                (
                    if matches!(transition, Transition::Failed(_)) {
                        RecordingProjectionState::Failed
                    } else {
                        RecordingProjectionState::Cancelled
                    },
                    vec![
                        RecordingProjectionState::Reserved,
                        RecordingProjectionState::Materializing,
                    ],
                    "failure_reason = $reason",
                    "state = $target, failure_reason = $reason",
                )
            }
        };
        let sql = format!(
            "BEGIN TRANSACTION;
             LET $current = (SELECT * FROM ONLY $projection WHERE {ADMITTED}
                 AND (state IN $predecessors OR (state = $target AND {repeat_matches})));
             IF $current = NONE {{ THROW 'recording_projection_transition_conflict'; }};
             IF $current.state != $target {{
                 UPDATE ONLY $projection SET {assignment}, updated_at = time::now() RETURN NONE;
             }};
             RETURN (SELECT * FROM ONLY $projection);
             COMMIT TRANSACTION;"
        );
        for attempt in 0..8 {
            let mut response = self
                .db
                .query(sql.clone())
                .bind(("projection", projection_id.record_id()))
                .bind(("recording", recording_id.record_id()))
                .bind(("scope", scope.bindings()))
                .bind(("grant_class", RecordingReadGrantClass::AppProjection))
                .bind(("target", target))
                .bind(("predecessors", predecessors.clone()))
                .bind(("bytes", transition.byte_len()))
                .bind(("digest", transition.digest().map(|v| v.hex().to_owned())))
                .bind(("reason", transition.reason().map(ToOwned::to_owned)))
                .await?;
            let Some(error) = primary_transaction_error(response.take_errors()) else {
                return response
                    .take::<Option<RecordingProjectionReceiptRecord>>(4)?
                    .ok_or(StoreError::MissingRecord {
                        operation: "recording projection transition",
                    });
            };
            if retryable(&error) && attempt < 7 {
                retry_delay(attempt).await;
                continue;
            }
            if error.is_thrown()
                && error
                    .message()
                    .contains("recording_projection_transition_conflict")
            {
                return Err(StoreError::RecordingProjectionConflict {
                    projection_id: projection_id.to_string(),
                });
            }
            return Err(error.into());
        }
        unreachable!("bounded transaction retry")
    }
}

#[derive(Clone, Copy)]
enum Transition<'a> {
    Begin,
    Complete {
        byte_len: i64,
        digest: &'a Sha256Digest,
    },
    Failed(&'a str),
    Cancelled(&'a str),
}
impl<'a> Transition<'a> {
    fn reason(self) -> Option<&'a str> {
        match self {
            Self::Failed(v) | Self::Cancelled(v) => Some(v),
            _ => None,
        }
    }
    fn byte_len(self) -> Option<i64> {
        match self {
            Self::Complete { byte_len, .. } => Some(byte_len),
            _ => None,
        }
    }
    fn digest(self) -> Option<&'a Sha256Digest> {
        match self {
            Self::Complete { digest, .. } => Some(digest),
            _ => None,
        }
    }
}
fn retryable(error: &surrealdb::Error) -> bool {
    matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    )
}
async fn retry_delay(attempt: u32) {
    tokio::time::sleep(Duration::from_millis(1 << attempt)).await;
}
fn request_error(error: surrealdb::Error) -> StoreError {
    if error.is_thrown()
        && error
            .message()
            .contains("recording_projection_request_conflict")
    {
        StoreError::RecordingProjectionRequestConflict
    } else {
        error.into()
    }
}
