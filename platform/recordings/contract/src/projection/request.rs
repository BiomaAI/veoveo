//! Flat JSON request admission and typed construction from an admitted query.

use std::{collections::BTreeMap, ops::Deref};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::{
    MAX_PROJECTION_DEADLINE_MS, RecordingProjectionQuery, RecordingProjectionQueryBuilder,
    RecordingProjectionSampling, RecordingProjectionSparseFill, query::valid_text,
};
use crate::{RecordingContractError, RecordingDatasetId, RecordingId};

/// Rust callers supply an admitted query; the wire representation keeps flat fields.
#[derive(Clone, Debug, Serialize)]
pub struct CreateRecordingProjectionRequestBuilder {
    pub dataset_id: RecordingDatasetId,
    pub recording_id: RecordingId,
    #[serde(flatten)]
    pub query: RecordingProjectionQuery,
    pub deadline_ms: u64,
    pub idempotency_key: String,
    pub units: BTreeMap<String, String>,
    pub coordinate_frame_refs: Vec<String>,
}

impl CreateRecordingProjectionRequestBuilder {
    pub fn build(self) -> Result<CreateRecordingProjectionRequest, RecordingContractError> {
        if !(1..=MAX_PROJECTION_DEADLINE_MS).contains(&self.deadline_ms)
            || !valid_text(&self.idempotency_key, 128)
            || self.units.len() > 64
            || !self
                .units
                .keys()
                .all(|key| self.query.component_ids.contains(key))
            || self.coordinate_frame_refs.len() > 64
            || !self
                .units
                .iter()
                .flat_map(|(key, value)| [key, value])
                .chain(&self.coordinate_frame_refs)
                .all(|value| valid_text(value, 256))
        {
            return Err(RecordingContractError::ProjectionMetadata);
        }
        Ok(CreateRecordingProjectionRequest(self))
    }
}

/// Request construction and decoding share the same bounds.
/// ```compile_fail
/// use veoveo_recording_contract::CreateRecordingProjectionRequest;
/// fn remove_deadline(request: &mut CreateRecordingProjectionRequest) {
///     request.deadline_ms = 0;
/// }
/// ```
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "ProjectionRequestWire")]
pub struct CreateRecordingProjectionRequest(CreateRecordingProjectionRequestBuilder);

impl Deref for CreateRecordingProjectionRequest {
    type Target = CreateRecordingProjectionRequestBuilder;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CreateRecordingProjectionRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ProjectionRequestWire::deserialize(deserializer)?;
        let query = RecordingProjectionQueryBuilder {
            entity_paths: wire.entity_paths,
            component_ids: wire.component_ids,
            timeline: wire.timeline,
            sampling: wire.sampling,
            sparse_fill: wire.sparse_fill,
            maximum_entities: wire.maximum_entities,
            maximum_columns: wire.maximum_columns,
            maximum_samples: wire.maximum_samples,
            maximum_rows: wire.maximum_rows,
            maximum_bytes: wire.maximum_bytes,
        }
        .build()
        .map_err(serde::de::Error::custom)?;
        CreateRecordingProjectionRequestBuilder {
            dataset_id: wire.dataset_id,
            recording_id: wire.recording_id,
            query,
            deadline_ms: wire.deadline_ms,
            idempotency_key: wire.idempotency_key,
            units: wire.units,
            coordinate_frame_refs: wire.coordinate_frame_refs,
        }
        .build()
        .map_err(serde::de::Error::custom)
    }
}

// A closed flat decoder avoids combining Serde flatten with deny_unknown_fields.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "CreateRecordingProjectionRequest")]
struct ProjectionRequestWire {
    dataset_id: RecordingDatasetId,
    recording_id: RecordingId,
    #[schemars(length(min = 1, max = 64))]
    entity_paths: Vec<String>,
    #[schemars(length(min = 1, max = 64))]
    component_ids: Vec<String>,
    timeline: String,
    sampling: RecordingProjectionSampling,
    sparse_fill: RecordingProjectionSparseFill,
    #[schemars(range(min = 1, max = 64))]
    maximum_entities: usize,
    #[schemars(range(min = 1, max = 64))]
    maximum_columns: usize,
    #[schemars(range(min = 1, max = 10000))]
    maximum_samples: usize,
    #[schemars(range(min = 1, max = 10000))]
    maximum_rows: u64,
    #[schemars(range(min = 1, max = 33554432))]
    maximum_bytes: u64,
    #[schemars(range(min = 1, max = 15000))]
    deadline_ms: u64,
    idempotency_key: String,
    units: BTreeMap<String, String>,
    coordinate_frame_refs: Vec<String>,
}
