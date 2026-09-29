//! Recording-owned catalog grants.

use schemars::JsonSchema;

use crate::{RecordingContractError, RecordingDatasetId, RecordingId, RecordingReadGrantId};
use serde::{Deserialize, Serialize};

pub const RECORDING_CATALOG_GRANT_SCHEMA: &str = "veoveo.ai/recording-catalog-grant/v1";

/// A bounded, sorted Recording selection. Dataset membership is checked by Store.
/// ```compile_fail
/// use veoveo_recording_contract::{CreateRecordingCatalogGrantRequest, RecordingId, RecordingDatasetId};
/// CreateRecordingCatalogGrantRequest::new(RecordingId::new(), vec![RecordingDatasetId::new()]);
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(try_from = "CatalogGrantRequestWire")]
#[schemars(with = "CatalogGrantRequestWire")]
pub struct CreateRecordingCatalogGrantRequest {
    dataset_id: RecordingDatasetId,
    recording_ids: Vec<RecordingId>,
}
impl CreateRecordingCatalogGrantRequest {
    pub fn new(
        dataset_id: RecordingDatasetId,
        mut recording_ids: Vec<RecordingId>,
    ) -> Result<Self, RecordingContractError> {
        if recording_ids.is_empty() || recording_ids.len() > 500 {
            return Err(RecordingContractError::Selection);
        }
        recording_ids.sort_unstable();
        recording_ids.dedup();
        Ok(Self {
            dataset_id,
            recording_ids,
        })
    }
    pub fn dataset_id(&self) -> RecordingDatasetId {
        self.dataset_id
    }
    pub fn recording_ids(&self) -> &[RecordingId] {
        &self.recording_ids
    }
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "CreateRecordingCatalogGrantRequest")]
struct CatalogGrantRequestWire {
    dataset_id: RecordingDatasetId,
    #[schemars(length(min = 1, max = 500))]
    recording_ids: Vec<RecordingId>,
}
impl TryFrom<CatalogGrantRequestWire> for CreateRecordingCatalogGrantRequest {
    type Error = RecordingContractError;
    fn try_from(wire: CatalogGrantRequestWire) -> Result<Self, Self::Error> {
        Self::new(wire.dataset_id, wire.recording_ids)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingCatalogGrant {
    pub schema: String,
    pub grant_id: RecordingReadGrantId,
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_ids: Vec<RecordingId>,
    pub catalog_revision: String,
    pub entry_uri: String,
    pub redap_token: String,
    pub expires_at: String,
}
