//! Projection result shape and agreement with the request that produced it.
use veoveo_types::sha256_hex;

use std::{collections::BTreeMap, num::NonZeroU64, ops::Deref};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use veoveo_frames_contract::WorldFrameUri;
use veoveo_types::Sha256Digest;

use super::{
    CreateRecordingProjectionRequest, MAX_PROJECTION_BYTES, MAX_PROJECTION_ROWS,
    MAX_PROJECTION_SAMPLES, MAX_PROJECTION_SELECTOR_BYTES, query::valid_text,
};
use crate::{RecordingContractError, RecordingDatasetId, RecordingId, RecordingProjectionId};

pub const RECORDING_PROJECTION_HANDLE_SCHEMA: &str = "veoveo.ai/recording-projection-handle/v1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
pub enum RecordingProjectionHandleSchema {
    #[serde(rename = "veoveo.ai/recording-projection-handle/v1")]
    V1,
}

/// Result facts supplied to the handle builder. File integrity is checked by the service.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingProjectionResultMetadata {
    pub catalog_revision: String,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub query_digest: Sha256Digest,
    pub timeline: String,
    pub sample_grid: Vec<i64>,
    pub units: BTreeMap<String, String>,
    #[schemars(length(max = 64))]
    pub coordinate_frame_refs: Vec<WorldFrameUri>,
    pub omitted_sample_count: u64,
    pub row_count: u64,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub arrow_schema_sha256: Sha256Digest,
    pub byte_len: NonZeroU64,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub payload_sha256: Sha256Digest,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "RecordingProjectionHandle")]
pub struct RecordingProjectionHandleBuilder {
    pub schema: RecordingProjectionHandleSchema,
    pub projection_id: RecordingProjectionId,
    pub dataset_id: RecordingDatasetId,
    pub recording_id: RecordingId,
    pub result: RecordingProjectionResultMetadata,
    #[schemars(with = "String")]
    pub expires_at: DateTime<Utc>,
}

impl RecordingProjectionHandleBuilder {
    pub fn build_for(
        self,
        request: &CreateRecordingProjectionRequest,
    ) -> Result<RecordingProjectionHandle, RecordingContractError> {
        let handle = self.admit()?;
        handle.validate_request(request)?;
        Ok(handle)
    }

    fn admit(self) -> Result<RecordingProjectionHandle, RecordingContractError> {
        let result = &self.result;
        let valid_samples = if result.sample_grid.is_empty() {
            result.omitted_sample_count == 0
        } else {
            result.sample_grid.len() <= MAX_PROJECTION_SAMPLES
                && result.sample_grid[0] > i64::MIN
                && result.sample_grid.windows(2).all(|pair| pair[0] < pair[1])
                && result.row_count.checked_add(result.omitted_sample_count)
                    == u64::try_from(result.sample_grid.len()).ok()
        };
        if !valid_samples
            || !valid_text(&result.catalog_revision, 128)
            || !valid_text(&result.timeline, MAX_PROJECTION_SELECTOR_BYTES)
            || result.byte_len.get() > MAX_PROJECTION_BYTES
            || result.row_count > MAX_PROJECTION_ROWS
            || result.row_count > MAX_PROJECTION_SAMPLES as u64
            || result.units.len() > 64
            || !super::valid_frame_references(&result.coordinate_frame_refs)
            || !result
                .units
                .iter()
                .flat_map(|(key, value)| [key, value])
                .all(|value| valid_text(value, 256))
        {
            return Err(RecordingContractError::ProjectionResult);
        }
        Ok(RecordingProjectionHandle(self))
    }
}

/// An immutable result with admitted shape. Consumers with the request also check agreement.
/// ```compile_fail
/// use veoveo_recording_contract::RecordingProjectionHandle;
/// fn change_length(handle: &mut RecordingProjectionHandle) {
///     handle.result.byte_len = 1.try_into().unwrap();
/// }
/// ```
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "RecordingProjectionHandleBuilder")]
pub struct RecordingProjectionHandle(RecordingProjectionHandleBuilder);

impl RecordingProjectionHandle {
    /// Check request identity, metadata and selected limits. This grants no source access.
    pub fn validate_request(
        &self,
        request: &CreateRecordingProjectionRequest,
    ) -> Result<(), RecordingContractError> {
        if self.dataset_id != request.dataset_id
            || self.recording_id != request.recording_id
            || self.result.timeline != request.query.timeline
            || self.result.sample_grid != request.query.sampling.sample_grid()
            || self.result.units != request.units
            || self.result.coordinate_frame_refs != request.coordinate_frame_refs
            || self.result.row_count > request.query.maximum_rows
            || self.result.row_count > request.query.maximum_samples as u64
            || self.result.byte_len.get() > request.query.maximum_bytes
        {
            return Err(RecordingContractError::ProjectionResult);
        }
        Ok(())
    }
}

impl Deref for RecordingProjectionHandle {
    type Target = RecordingProjectionHandleBuilder;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RecordingProjectionHandle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        RecordingProjectionHandleBuilder::deserialize(deserializer)?
            .admit()
            .map_err(serde::de::Error::custom)
    }
}
