//! Map-owned addresses for immutable releases and their geographic products.

use veoveo_types::ResourceUri;

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
#[veoveo_types::resource_address(cached(MapProductUriErrorAddresses), template = "map://dataset/{dataset_id}/release/{release_id}", traits = ordered, schema = string)]
pub struct MapReleaseUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "dataset_id", error = |_| MapProductUriError, accessor = dataset_id)]
    dataset_id: MapDatasetId,
    #[resource(variable = "release_id", error = |_| MapProductUriError, accessor = release_id)]
    release_id: DatasetReleaseId,
}
impl MapReleaseUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached(MapProductUriErrorAddresses), template = "map://source-feature/{release_id}/{source_feature_id}", traits = ordered, schema = string)]
pub struct MapSourceFeatureUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "release_id", error = |_| MapProductUriError, accessor = release_id)]
    release_id: DatasetReleaseId,
    #[resource(variable = "source_feature_id", error = |_| MapProductUriError, accessor = feature_id)]
    feature_id: SourceFeatureId,
}
impl MapSourceFeatureUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}

/// ```compile_fail
/// use veoveo_map_mcp::contract::{MapRouteUri, RasterProductId};
/// MapRouteUri::new(RasterProductId::new());
/// ```
#[veoveo_types::resource_address(cached(MapProductUriErrorAddresses), template = "map://route/{route_id}", traits = ordered, schema = string)]
pub struct MapRouteUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "route_id", error = |_| MapProductUriError, accessor = id)]
    id: RouteId,
}
impl MapRouteUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached(MapProductUriErrorAddresses), template = "map://raster/{raster_id}", traits = ordered, schema = string)]
pub struct MapRasterUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "raster_id", error = |_| MapProductUriError, accessor = id)]
    id: RasterProductId,
}
impl MapRasterUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached(MapProductUriErrorAddresses), template = "map://raster-derivation/{raster_derivation_id}", traits = ordered, schema = string)]
pub struct MapRasterDerivationUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "raster_derivation_id", error = |_| MapProductUriError, accessor = id)]
    id: RasterDerivationId,
}
impl MapRasterDerivationUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached(MapProductUriErrorAddresses), template = "map://spatial-derivation/{spatial_derivation_id}", traits = ordered, schema = string)]
pub struct MapSpatialDerivationUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "spatial_derivation_id", error = |_| MapProductUriError, accessor = id)]
    id: SpatialDerivationId,
}
impl MapSpatialDerivationUri {
    pub const TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE;
}

#[doc(hidden)]
pub struct MapProductUriErrorAddresses;
impl veoveo_types::ResourceProfile for MapProductUriErrorAddresses {
    type Error = MapProductUriError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| MapProductUriError,
        };
}
