use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{AccessSubject, DataLabelId, PrincipalId, WorkContextId};

use super::{
    FeatureLayerId, LayerProductId, LayerPublicationId, MapCompositionId, MapCompositionRevisionId,
    StyleRevisionId, Wgs84Position,
};

pub const MAX_COMPOSITION_LAYERS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayerProductFormat {
    GeoJsonSeq,
    GeoParquet,
    GeoPackage,
    MvtBundle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "LayerProduct")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct LayerProductValue {
    pub product_id: LayerProductId,
    pub publication_id: LayerPublicationId,
    pub layer_id: FeatureLayerId,
    pub layer_revision: u64,
    pub format: LayerProductFormat,
    pub artifact_uri: veoveo_artifact_contract::ArtifactUri,
    pub mime_type: String,
    pub digest_sha256: String,
    pub size_bytes: u64,
    pub feature_count: u64,
    pub created_by: PrincipalId,
    pub work_context: WorkContextId,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CompositionView {
    pub center: Wgs84Position,
    pub zoom: f64,
    #[serde(default)]
    pub bearing_deg: f64,
    #[serde(default)]
    pub pitch_deg: f64,
}

impl CompositionView {
    pub fn validate(&self) -> Result<(), String> {
        self.center.validate().map_err(|error| error.to_string())?;
        if !self.zoom.is_finite() || !(-2.0..=24.0).contains(&self.zoom) {
            return Err("composition zoom must be within [-2, 24]".to_owned());
        }
        if !self.bearing_deg.is_finite() || !(-180.0..=180.0).contains(&self.bearing_deg) {
            return Err("composition bearing must be within [-180, 180]".to_owned());
        }
        if !self.pitch_deg.is_finite() || !(0.0..=85.0).contains(&self.pitch_deg) {
            return Err("composition pitch must be within [0, 85]".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CompositionLayer {
    pub layer_id: FeatureLayerId,
    pub publication_id: LayerPublicationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_revision_id: Option<StyleRevisionId>,
    #[serde(default = "default_visible")]
    pub visible: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

fn default_visible() -> bool {
    true
}

fn default_opacity() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "MapCompositionRevision")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct MapCompositionRevisionValue {
    pub composition_revision_id: MapCompositionRevisionId,
    pub composition_id: MapCompositionId,
    pub revision: u64,
    pub layers: Vec<CompositionLayer>,
    pub view: CompositionView,
    pub created_by: PrincipalId,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "MapComposition")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct MapCompositionValue {
    pub composition_id: MapCompositionId,
    pub title: String,
    pub current: MapCompositionRevision,
    pub owner: AccessSubject,
    pub created_by: PrincipalId,
    pub work_context: WorkContextId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataLabelId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CreateMapCompositionRequest {
    pub title: String,
    pub layers: Vec<CompositionLayer>,
    pub view: CompositionView,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMapCompositionRequest {
    pub composition_id: MapCompositionId,
    pub expected_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub layers: Vec<CompositionLayer>,
    pub view: CompositionView,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMapCompositionRequest {
    pub composition_id: MapCompositionId,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayerProduct(veoveo_types::Checked<LayerProductValue>);
impl LayerProduct {
    pub fn new(value: LayerProductValue) -> Result<Self, super::MapRelationshipError> {
        veoveo_types::Checked::new(value).map(Self)
    }
    pub fn into_value(self) -> LayerProductValue {
        self.0.into_inner()
    }
}
impl std::ops::Deref for LayerProduct {
    type Target = LayerProductValue;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl veoveo_types::Check for LayerProductValue {
    type Error = super::MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        super::relationships::check_product(self)
    }
}
impl JsonSchema for LayerProduct {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        LayerProductValue::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        LayerProductValue::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        LayerProductValue::json_schema(generator)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MapCompositionRevision(veoveo_types::Checked<MapCompositionRevisionValue>);
impl MapCompositionRevision {
    pub fn new(value: MapCompositionRevisionValue) -> Result<Self, super::MapRelationshipError> {
        veoveo_types::Checked::new(value).map(Self)
    }
    pub fn into_value(self) -> MapCompositionRevisionValue {
        self.0.into_inner()
    }
}
impl std::ops::Deref for MapCompositionRevision {
    type Target = MapCompositionRevisionValue;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl veoveo_types::Check for MapCompositionRevisionValue {
    type Error = super::MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        super::relationships::check_composition_revision(self)
    }
}
impl JsonSchema for MapCompositionRevision {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        MapCompositionRevisionValue::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        MapCompositionRevisionValue::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        MapCompositionRevisionValue::json_schema(generator)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MapComposition(veoveo_types::Checked<MapCompositionValue>);
impl MapComposition {
    pub fn new(value: MapCompositionValue) -> Result<Self, super::MapRelationshipError> {
        veoveo_types::Checked::new(value).map(Self)
    }
    pub fn into_value(self) -> MapCompositionValue {
        self.0.into_inner()
    }
}
impl std::ops::Deref for MapComposition {
    type Target = MapCompositionValue;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl veoveo_types::Check for MapCompositionValue {
    type Error = super::MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        super::relationships::check_composition(self)
    }
}
impl JsonSchema for MapComposition {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        MapCompositionValue::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        MapCompositionValue::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        MapCompositionValue::json_schema(generator)
    }
}
