//! Public catalog views admitted after the service selects current visible records.
use crate::{
    RecordingCatalogCursor, RecordingContractError, RecordingDatasetId, RecordingId,
    RecordingState, checked::text,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactUri;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(rename = "RecordingView")]
pub struct RecordingViewBuilder {
    pub recording_id: RecordingId,
    pub dataset_id: RecordingDatasetId,
    pub dataset_key: String,
    pub application_id: String,
    pub recording_key: String,
    pub state: RecordingState,
    pub classification: String,
    pub labels: Vec<String>,
    #[schemars(with = "String", extend("format" = "date-time"))]
    pub started_at: DateTime<Utc>,
    #[schemars(with = "String", extend("format" = "date-time"))]
    pub last_data_at: DateTime<Utc>,
    #[schemars(with = "Option<String>", extend("format" = "date-time"))]
    pub ended_at: Option<DateTime<Utc>>,
    #[schemars(with = "Option<String>", extend("format" = "date-time"))]
    pub sealed_at: Option<DateTime<Utc>>,
    pub manifest_artifact_uri: Option<ArtifactUri>,
    /// Counts cover the current catalog, including later Derived lifecycles.
    pub layer_count: usize,
    pub committed_layer_count: usize,
}
impl RecordingViewBuilder {
    pub fn build(self) -> Result<RecordingView, RecordingContractError> {
        veoveo_types::Checked::new(self).map(RecordingView)
    }
}
impl veoveo_types::Check for RecordingViewBuilder {
    type Error = RecordingContractError;
    fn check(&self) -> Result<(), Self::Error> {
        if !text(&self.dataset_key, 128)
            || !text(&self.application_id, 512)
            || !text(&self.recording_key, 512)
            || !text(&self.classification, 256)
            || self.labels.iter().any(|label| !text(label, 256))
            || self.labels.len() > 128
            || self.labels.windows(2).any(|pair| pair[0] >= pair[1])
            || self.last_data_at < self.started_at
            || self.ended_at.is_some_and(|end| end < self.started_at)
            || self.committed_layer_count > self.layer_count
            || self
                .manifest_artifact_uri
                .as_ref()
                .is_some_and(|uri| uri.artifact_id().as_uuid() != self.recording_id.as_uuid())
            || (self.state == RecordingState::Live
                && (self.ended_at.is_some()
                    || self.sealed_at.is_some()
                    || self.manifest_artifact_uri.is_some()))
            || (self.state == RecordingState::Sealed
                && (self.ended_at.is_none()
                    || self.sealed_at.is_none()
                    || self.manifest_artifact_uri.is_none()
                    || self.layer_count == 0
                    || self.committed_layer_count == 0))
            || (self.sealed_at.is_some() && self.state != RecordingState::Sealed)
        {
            return Err(RecordingContractError::CatalogView);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct RecordingView(veoveo_types::Checked<RecordingViewBuilder>);
impl std::ops::Deref for RecordingView {
    type Target = RecordingViewBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordingCatalogPage {
    pub items: Vec<RecordingView>,
    pub limit: usize,
    pub next_cursor: Option<RecordingCatalogCursor>,
}
