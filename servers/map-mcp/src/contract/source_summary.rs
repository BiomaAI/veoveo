//! Public source metadata excludes acquisition endpoints and secret references.
use super::{
    AcquisitionModel, AuthorityClass, DatasetLicense, MapDatasetId, MapFamily, MapSourceError,
    MapSourceId, MapSourceUri, RegisteredSource, SourceAdapterKind,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SummaryWire", into = "SummaryWire")]
pub struct SourceSummary(veoveo_types::Checked<SummaryWire>);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SummaryWire {
    source_id: MapSourceId,
    dataset_id: MapDatasetId,
    #[schemars(length(min = 1, max = 256))]
    name: String,
    adapter_kind: SourceAdapterKind,
    authority: AuthorityClass,
    acquisition_model: AcquisitionModel,
    #[schemars(length(min = 1))]
    map_families: BTreeSet<MapFamily>,
    license: DatasetLicense,
    enabled: bool,
    #[schemars(range(min = 1))]
    record_version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SourceSummary {
    pub fn new(source: &RegisteredSource) -> Result<Self, MapSourceError> {
        SummaryWire {
            source_id: source.source_id.clone(),
            dataset_id: source.dataset_id.clone(),
            name: source.name.clone(),
            adapter_kind: source.adapter_kind,
            authority: source.authority,
            acquisition_model: source.acquisition_model,
            map_families: source.map_families.clone(),
            license: source.license.clone(),
            enabled: source.enabled,
            record_version: source.record_version,
            created_at: source.created_at,
            updated_at: source.updated_at,
        }
        .try_into()
    }
    pub fn source_id(&self) -> &MapSourceId {
        &self.0.source_id
    }
    pub fn resource_uri(&self) -> MapSourceUri {
        MapSourceUri::new(self.0.source_id.clone())
    }
    pub fn dataset_id(&self) -> &MapDatasetId {
        &self.0.dataset_id
    }
    pub fn name(&self) -> &str {
        &self.0.name
    }
    pub fn adapter_kind(&self) -> SourceAdapterKind {
        self.0.adapter_kind
    }
    pub fn authority(&self) -> AuthorityClass {
        self.0.authority
    }
    pub fn acquisition_model(&self) -> AcquisitionModel {
        self.0.acquisition_model
    }
    pub fn map_families(&self) -> &BTreeSet<MapFamily> {
        &self.0.map_families
    }
    pub fn license(&self) -> &DatasetLicense {
        &self.0.license
    }
    pub fn enabled(&self) -> bool {
        self.0.enabled
    }
    pub fn record_version(&self) -> u64 {
        self.0.record_version
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.0.updated_at
    }
}
impl veoveo_types::Check for SummaryWire {
    type Error = MapSourceError;
    fn check(&self) -> Result<(), Self::Error> {
        let wire = self;
        super::datasets::validate_controlled(&wire.name, 256)
            .map_err(|_| MapSourceError::Metadata)?;
        wire.license
            .validate()
            .map_err(|_| MapSourceError::Metadata)?;
        if wire.map_families.is_empty()
            || wire.record_version == 0
            || wire.updated_at < wire.created_at
        {
            return Err(MapSourceError::Metadata);
        }
        Ok(())
    }
}
impl TryFrom<SummaryWire> for SourceSummary {
    type Error = MapSourceError;
    fn try_from(value: SummaryWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<SourceSummary> for SummaryWire {
    fn from(summary: SourceSummary) -> Self {
        summary.0.into_inner()
    }
}
