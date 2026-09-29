//! Map-owned addresses for immutable releases and their geographic products.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

use super::{
    DatasetReleaseId, MapDatasetId, RasterDerivationId, RasterProductId, RouteId, SourceFeatureId,
    SpatialDerivationId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Map product address or identity")]
pub struct MapProductUriError;

fn id<T: std::str::FromStr>(value: &str) -> Result<T, MapProductUriError> {
    value.parse().map_err(|_| MapProductUriError)
}

macro_rules! address {
    ($(#[$meta:meta])* $name:ident, $root:literal, $template:literal,
     { $($field:ident: $ty:ty),+ }, $segments:expr, $parse:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name {
            wire: ResourceUri,
            $($field: $ty),+
        }
        impl $name {
            pub const TEMPLATE: &str = $template;
            pub fn new($($field: $ty),+) -> Self {
                let mut builder = ResourceUriBuilder::new(concat!("map://", $root))
                    .expect("declared Map product root");
                for segment in $segments {
                    builder = builder.segment(UriSegment::new(segment).expect("typed Map product component"));
                }
                Self { wire: builder.build().expect("typed Map product address"), $($field),+ }
            }
            pub fn parse(value: impl AsRef<str>) -> Result<Self, MapProductUriError> {
                let value = value.as_ref();
                let parts = ResourceUriParts::parse(value).map_err(|_| MapProductUriError)?;
                if parts.scheme() != "map" || parts.authority() != $root || parts.has_query() {
                    return Err(MapProductUriError);
                }
                let path = parts.path_segments().map(|part| part.into_owned()).collect::<Vec<_>>();
                let ($($field,)+) = ($parse)(&path)?;
                let address = Self::new($($field),+);
                if address.as_str() != value { return Err(MapProductUriError); }
                Ok(address)
            }
            $(pub fn $field(&self) -> &$ty { &self.$field })+
            pub fn as_str(&self) -> &str { self.wire.as_str() }
        }
        impl TryFrom<String> for $name {
            type Error = MapProductUriError;
            fn try_from(value: String) -> Result<Self, Self::Error> { Self::parse(value) }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self { value.wire.to_string() }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.as_str()) }
        }
        impl ResourceAddress for $name {
            type Error = MapProductUriError;
            fn parse(value: &ResourceUri) -> Result<Self, Self::Error> { Self::parse(value.as_str()) }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> { Ok(self.wire.clone()) }
        }
    }
}

address!(
    /// One release under its owning dataset.
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapReleaseUri, DatasetReleaseId, MapDatasetId};
    /// MapReleaseUri::new(DatasetReleaseId::new(), MapDatasetId::new());
    /// ```
    MapReleaseUri, "dataset", "map://dataset/{dataset_id}/release/{release_id}",
    { dataset_id: MapDatasetId, release_id: DatasetReleaseId },
    [dataset_id.as_str(), "release", release_id.as_str()],
    |path: &[String]| -> Result<_, MapProductUriError> {
        match path {
            [dataset, marker, release] if marker == "release" => Ok((id(dataset)?, id(release)?)),
            _ => Err(MapProductUriError),
        }
    }
);
address!(
    MapSourceFeatureUri, "source-feature", "map://source-feature/{release_id}/{source_feature_id}",
    { release_id: DatasetReleaseId, feature_id: SourceFeatureId },
    [release_id.as_str(), feature_id.as_str()],
    |path: &[String]| -> Result<_, MapProductUriError> {
        match path {
            [release, feature] => Ok((id(release)?, id(feature)?)),
            _ => Err(MapProductUriError),
        }
    }
);

macro_rules! single_address {
    ($(#[$meta:meta])* $name:ident, $root:literal, $template:literal, $ty:ty) => {
        address!($(#[$meta])* $name, $root, $template, { id: $ty }, [id.as_str()],
            |path: &[String]| -> Result<_, MapProductUriError> {
                match path {
                    [value] => Ok((id(value)?,)),
                    _ => Err(MapProductUriError),
                }
            }
        );
    }
}
single_address!(
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapRouteUri, RasterProductId};
    /// MapRouteUri::new(RasterProductId::new());
    /// ```
    MapRouteUri, "route", "map://route/{route_id}", RouteId
);
single_address!(
    MapRasterUri,
    "raster",
    "map://raster/{raster_id}",
    RasterProductId
);
single_address!(
    MapRasterDerivationUri,
    "raster-derivation",
    "map://raster-derivation/{raster_derivation_id}",
    RasterDerivationId
);
single_address!(
    MapSpatialDerivationUri,
    "spatial-derivation",
    "map://spatial-derivation/{spatial_derivation_id}",
    SpatialDerivationId
);
