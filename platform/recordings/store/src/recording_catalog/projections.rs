//! Recording projection admission and atomic receipt lifecycle.
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_types::Sha256Digest;

use super::{RecordingAccessScope, validate_text};
use crate::{
    RecordingDatasetId, RecordingId, RecordingProjectionReceiptId,
    RecordingProjectionReceiptRecord, RecordingProjectionState, RecordingReadGrantClass,
    RecordingReadGrantId, RecordingRepository, RecordingStoreError,
};
use veoveo_platform_store::primary_transaction_error;

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
    ) -> Result<Self, RecordingStoreError> {
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
    pub scope: RecordingAccessScope,
    pub request: RecordingProjectionRequest,
    pub grant_id: RecordingReadGrantId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, SurrealValue)]
struct RequestBindings {
    dataset: RecordId,
    key: String,
    manifest: String,
    query: String,
}

impl RecordingRepository {
    pub async fn ready_recording_projection(
        &self,
        scope: &RecordingAccessScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, RecordingStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/recording_catalog/projections/ready_recording_projection.surql"
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
        scope: &RecordingAccessScope,
        request: &RecordingProjectionRequest,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, RecordingStoreError> {
        let mut response = self.client()
            .query(include_str!("../queries/recording_catalog/projections/recording_projection_by_idempotency_key.surql"))
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
    ) -> Result<RecordingProjectionReceiptRecord, RecordingStoreError> {
        let id = RecordingProjectionReceiptId::new();
        let sql = include_str!(
            "../queries/recording_catalog/projections/reserve_recording_projection.surql"
        );
        for attempt in 0..8 {
            let mut response = self
                .client()
                .query(sql)
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
                    .ok_or(RecordingStoreError::MissingRecord {
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
                return Err(RecordingStoreError::RecordingReadGrantConflict {
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
        scope: &RecordingAccessScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<RecordingProjectionReceiptRecord, RecordingStoreError> {
        self.transition_projection(scope, recording_id, projection_id, Transition::Begin)
            .await
    }

    pub async fn complete_recording_projection(
        &self,
        scope: &RecordingAccessScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
        result_byte_len: u64,
        result_sha256: &Sha256Digest,
    ) -> Result<RecordingProjectionReceiptRecord, RecordingStoreError> {
        let byte_len = i64::try_from(result_byte_len).map_err(|_| {
            RecordingStoreError::InvalidRecordingField {
                field: "projection result_byte_len",
                reason: "must fit a signed 64-bit byte count",
            }
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
        scope: &RecordingAccessScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, RecordingStoreError> {
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
        scope: &RecordingAccessScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, RecordingStoreError> {
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
        scope: &RecordingAccessScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
        transition: Transition<'_>,
    ) -> Result<RecordingProjectionReceiptRecord, RecordingStoreError> {
        let (target, predecessors, sql) = match transition {
            Transition::Begin => (
                RecordingProjectionState::Materializing,
                vec![RecordingProjectionState::Reserved],
                include_str!("../queries/recording_catalog/projections/transition_begin.surql"),
            ),
            Transition::Complete { .. } => (
                RecordingProjectionState::Ready,
                vec![RecordingProjectionState::Materializing],
                include_str!("../queries/recording_catalog/projections/transition_complete.surql"),
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
                    include_str!(
                        "../queries/recording_catalog/projections/transition_terminal.surql"
                    ),
                )
            }
        };
        for attempt in 0..8 {
            let mut response = self
                .client()
                .query(sql)
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
                    .ok_or(RecordingStoreError::MissingRecord {
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
                return Err(RecordingStoreError::RecordingProjectionConflict {
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
fn request_error(error: surrealdb::Error) -> RecordingStoreError {
    if error.is_thrown()
        && error
            .message()
            .contains("recording_projection_request_conflict")
    {
        RecordingStoreError::RecordingProjectionRequestConflict
    } else {
        error.into()
    }
}
