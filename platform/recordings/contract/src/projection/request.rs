//! Flat JSON request admission and typed construction from an admitted query.

use std::{collections::BTreeMap, ops::Deref};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use veoveo_frames_contract::WorldFrameUri;

use super::{
    MAX_PROJECTION_DEADLINE_MS, RecordingProjectionQuery, RecordingProjectionQueryBuilder,
    RecordingProjectionSampling, RecordingProjectionSparseFill, query::valid_text,
};
use crate::{RecordingContractError, RecordingDatasetId, RecordingId};

/// Rust callers supply an admitted query; the wire representation keeps flat fields.
/// Coordinate-frame references require the Frames owner's immutable revision address.
/// ```compile_fail
/// use veoveo_recording_contract::CreateRecordingProjectionRequestBuilder;
/// fn replace_frames(builder: &mut CreateRecordingProjectionRequestBuilder) {
///     builder.coordinate_frame_refs = vec!["frames://world/unpinned".to_owned()];
/// }
/// ```
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecordingProjectionRequestBuilder {
    pub dataset_id: RecordingDatasetId,
    pub recording_id: RecordingId,
    #[serde(flatten)]
    pub query: RecordingProjectionQuery,
    pub deadline_ms: u64,
    pub idempotency_key: String,
    pub units: BTreeMap<String, String>,
    pub coordinate_frame_refs: Vec<WorldFrameUri>,
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
            || !super::valid_frame_references(&self.coordinate_frame_refs)
            || !self
                .units
                .iter()
                .flat_map(|(key, value)| [key, value])
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

impl CreateRecordingProjectionRequest {
    /// Deterministic identity input, excluding only the caller's idempotency key.
    /// The owner defines its field order; this is not a general JSON canonicalization profile.
    pub fn query_identity(&self) -> impl Serialize + '_ {
        // Exhaustive destructuring makes a new request field require an identity decision.
        let CreateRecordingProjectionRequestBuilder {
            dataset_id,
            recording_id,
            query,
            deadline_ms,
            idempotency_key: _,
            units,
            coordinate_frame_refs,
        } = &self.0;
        ProjectionQueryIdentity {
            dataset_id,
            recording_id,
            query,
            deadline_ms: *deadline_ms,
            units,
            coordinate_frame_refs,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectionQueryIdentity<'a> {
    dataset_id: &'a RecordingDatasetId,
    recording_id: &'a RecordingId,
    #[serde(flatten)]
    query: &'a RecordingProjectionQuery,
    deadline_ms: u64,
    units: &'a BTreeMap<String, String>,
    coordinate_frame_refs: &'a [WorldFrameUri],
}

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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
    #[schemars(schema_with = "crate::projection_units_schema")]
    units: BTreeMap<String, String>,
    #[schemars(length(max = 64))]
    coordinate_frame_refs: Vec<WorldFrameUri>,
}
