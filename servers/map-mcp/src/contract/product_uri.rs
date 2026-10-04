//! Map-owned addresses for immutable releases and their geographic products.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri};

use super::{
    DatasetReleaseId, MapDatasetId, RasterDerivationId, RasterProductId, RouteId, SourceFeatureId,
    SpatialDerivationId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Map product address or identity")]
pub struct MapProductUriError;

/// One release under its owning dataset.
/// ```compile_fail
/// use veoveo_map_mcp::contract::{MapReleaseUri, DatasetReleaseId, MapDatasetId};
/// MapReleaseUri::new(DatasetReleaseId::new(), MapDatasetId::new());
/// ```
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://dataset/{dataset_id}/release/{release_id}", error = MapProductUriError, route_error = |_| MapProductUriError, wire)]
pub struct MapReleaseUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "dataset_id", error = |_| MapProductUriError)]
    dataset_id: MapDatasetId,
    #[resource(variable = "release_id", error = |_| MapProductUriError)]
    release_id: DatasetReleaseId,
}
impl MapReleaseUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(dataset_id: MapDatasetId, release_id: DatasetReleaseId) -> Self {
        Self::resource_from_parts(dataset_id, release_id).expect("typed Map product address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapProductUriError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapProductUriError)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn dataset_id(&self) -> &MapDatasetId {
        &self.dataset_id
    }
    pub fn release_id(&self) -> &DatasetReleaseId {
        &self.release_id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl std::fmt::Display for MapReleaseUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://source-feature/{release_id}/{source_feature_id}", error = MapProductUriError, route_error = |_| MapProductUriError, wire)]
pub struct MapSourceFeatureUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "release_id", error = |_| MapProductUriError)]
    release_id: DatasetReleaseId,
    #[resource(variable = "source_feature_id", error = |_| MapProductUriError)]
    feature_id: SourceFeatureId,
}
impl MapSourceFeatureUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(release_id: DatasetReleaseId, feature_id: SourceFeatureId) -> Self {
        Self::resource_from_parts(release_id, feature_id).expect("typed Map product address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapProductUriError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapProductUriError)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn release_id(&self) -> &DatasetReleaseId {
        &self.release_id
    }
    pub fn feature_id(&self) -> &SourceFeatureId {
        &self.feature_id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl std::fmt::Display for MapSourceFeatureUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// ```compile_fail
/// use veoveo_map_mcp::contract::{MapRouteUri, RasterProductId};
/// MapRouteUri::new(RasterProductId::new());
/// ```
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://route/{route_id}", error = MapProductUriError, route_error = |_| MapProductUriError, wire)]
pub struct MapRouteUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "route_id", error = |_| MapProductUriError)]
    id: RouteId,
}
impl MapRouteUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(id: RouteId) -> Self {
        Self::resource_from_parts(id).expect("typed Map product address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapProductUriError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapProductUriError)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &RouteId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl std::fmt::Display for MapRouteUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://raster/{raster_id}", error = MapProductUriError, route_error = |_| MapProductUriError, wire)]
pub struct MapRasterUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "raster_id", error = |_| MapProductUriError)]
    id: RasterProductId,
}
impl MapRasterUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(id: RasterProductId) -> Self {
        Self::resource_from_parts(id).expect("typed Map product address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapProductUriError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapProductUriError)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &RasterProductId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl std::fmt::Display for MapRasterUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://raster-derivation/{raster_derivation_id}", error = MapProductUriError, route_error = |_| MapProductUriError, wire)]
pub struct MapRasterDerivationUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "raster_derivation_id", error = |_| MapProductUriError)]
    id: RasterDerivationId,
}
impl MapRasterDerivationUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(id: RasterDerivationId) -> Self {
        Self::resource_from_parts(id).expect("typed Map product address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapProductUriError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapProductUriError)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &RasterDerivationId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl std::fmt::Display for MapRasterDerivationUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://spatial-derivation/{spatial_derivation_id}", error = MapProductUriError, route_error = |_| MapProductUriError, wire)]
pub struct MapSpatialDerivationUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "spatial_derivation_id", error = |_| MapProductUriError)]
    id: SpatialDerivationId,
}
impl MapSpatialDerivationUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
    pub fn new(id: SpatialDerivationId) -> Self {
        Self::resource_from_parts(id).expect("typed Map product address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapProductUriError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapProductUriError)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &SpatialDerivationId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl std::fmt::Display for MapSpatialDerivationUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
