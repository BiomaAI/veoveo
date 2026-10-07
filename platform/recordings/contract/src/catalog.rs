//! Recording-owned catalog grants.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use std::ops::Deref;

use crate::{
    RecordingCatalogUri, RecordingContractError, RecordingDatasetId, RecordingId,
    RecordingReadGrantId,
};
use serde::{Deserialize, Deserializer, Serialize};

pub const RECORDING_CATALOG_GRANT_SCHEMA: &str = "veoveo.ai/recording-catalog-grant/v2";

/// A bounded, sorted Recording selection. Dataset membership is checked by Store.
/// ```compile_fail
/// use veoveo_recording_contract::{CreateRecordingCatalogGrantRequest, RecordingId, RecordingDatasetId};
/// CreateRecordingCatalogGrantRequest::new(RecordingId::new(), vec![RecordingDatasetId::new()]);
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(try_from = "CatalogGrantRequestWire", rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[schemars(transform = crate::format_tag_role)]
pub enum RecordingCatalogGrantSchema {
    #[serde(rename = "veoveo.ai/recording-catalog-grant/v2")]
    V2,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(rename = "RecordingCatalogGrant")]
pub struct RecordingCatalogGrantBuilder {
    pub schema: RecordingCatalogGrantSchema,
    pub grant_id: RecordingReadGrantId,
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_ids: Vec<RecordingId>,
    pub catalog_revision: String,
    pub entry_uri: RecordingCatalogUri,
    pub redap_token: String,
    #[schemars(with = "String", extend("format" = "date-time"))]
    pub expires_at: DateTime<Utc>,
}

impl RecordingCatalogGrantBuilder {
    pub fn build(self) -> Result<RecordingCatalogGrant, RecordingContractError> {
        veoveo_types::Checked::new(self).map(RecordingCatalogGrant)
    }
}
impl veoveo_types::Check for RecordingCatalogGrantBuilder {
    type Error = RecordingContractError;
    fn check(&self) -> Result<(), Self::Error> {
        let text = |value: &str| !value.trim().is_empty() && !value.chars().any(char::is_control);
        if self.entry_uri.dataset_id() != self.dataset_id
            || !(1..=500).contains(&self.recording_segment_ids.len())
            || !self
                .recording_segment_ids
                .windows(2)
                .all(|ids| ids[0] < ids[1])
            || !text(&self.catalog_revision)
            || self.catalog_revision.len() > 128
            || !text(&self.redap_token)
        {
            return Err(RecordingContractError::CatalogGrant);
        }
        Ok(())
    }
}

/// An admitted catalog response. Authorization and token verification belong to the service.
/// ```compile_fail
/// use veoveo_recording_contract::{RecordingCatalogGrant, RecordingDatasetId};
/// fn change_parent(grant: &mut RecordingCatalogGrant) {
///     grant.dataset_id = RecordingDatasetId::new();
/// }
/// ```
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "RecordingCatalogGrantBuilder")]
pub struct RecordingCatalogGrant(veoveo_types::Checked<RecordingCatalogGrantBuilder>);

impl Deref for RecordingCatalogGrant {
    type Target = RecordingCatalogGrantBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

impl<'de> Deserialize<'de> for RecordingCatalogGrant {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        veoveo_types::Checked::deserialize(deserializer).map(Self)
    }
}
