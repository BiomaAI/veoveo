//! Fixed discovery and direct Map resources. Paged collections and filtered feature
//! queries use their own contract types. All identities remain domain-owned.
use super::{
    AcquisitionId, FacilityId, FeatureChangeSetId, FeatureLayerId, LayerProductId,
    LayerPublicationId, LocationId, MapCompositionId, MapDatasetId, MapFeatureId, RouteMatrixId,
    StyleRevisionId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Map resource address or identity")]
pub struct MapResourceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum MapDocument {
    Resources,
    Agents,
    Design,
    Authoring,
    Acquisition,
    Routing,
}
impl MapDocument {
    pub fn parse(value: &str) -> Result<Self, MapResourceError> {
        match value {
            "resources" => Ok(Self::Resources),
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            "authoring" => Ok(Self::Authoring),
            "acquisition" => Ok(Self::Acquisition),
            "routing" => Ok(Self::Routing),
            _ => Err(MapResourceError),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resources => "resources",
            Self::Agents => "agents",
            Self::Design => "design",
            Self::Authoring => "authoring",
            Self::Acquisition => "acquisition",
            Self::Routing => "routing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum MapRoot {
    Docs,
    Contract,
    Sources,
    Acquisitions,
    Datasets,
    ActiveReleases,
    Locations,
    Facilities,
    MobilityProfiles,
    Restrictions,
    Routes,
    Matrices,
    TravelModels,
    Rasters,
    RasterDerivations,
    SpatialDerivations,
    FeatureLayers,
    Publications,
    LayerProducts,
    Compositions,
    Workspace,
}
impl MapRoot {
    pub fn parse(value: &str) -> Result<Self, MapResourceError> {
        match value {
            "docs" => Ok(Self::Docs),
            "contract" => Ok(Self::Contract),
            "sources" => Ok(Self::Sources),
            "acquisitions" => Ok(Self::Acquisitions),
            "datasets" => Ok(Self::Datasets),
            "active-releases" => Ok(Self::ActiveReleases),
            "locations" => Ok(Self::Locations),
            "facilities" => Ok(Self::Facilities),
            "mobility-profiles" => Ok(Self::MobilityProfiles),
            "restrictions" => Ok(Self::Restrictions),
            "routes" => Ok(Self::Routes),
            "matrices" => Ok(Self::Matrices),
            "travel-models" => Ok(Self::TravelModels),
            "rasters" => Ok(Self::Rasters),
            "raster-derivations" => Ok(Self::RasterDerivations),
            "spatial-derivations" => Ok(Self::SpatialDerivations),
            "feature-layers" => Ok(Self::FeatureLayers),
            "publications" => Ok(Self::Publications),
            "layer-products" => Ok(Self::LayerProducts),
            "compositions" => Ok(Self::Compositions),
            "workspace" => Ok(Self::Workspace),
            _ => Err(MapResourceError),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Docs => "docs",
            Self::Contract => "contract",
            Self::Sources => "sources",
            Self::Acquisitions => "acquisitions",
            Self::Datasets => "datasets",
            Self::ActiveReleases => "active-releases",
            Self::Locations => "locations",
            Self::Facilities => "facilities",
            Self::MobilityProfiles => "mobility-profiles",
            Self::Restrictions => "restrictions",
            Self::Routes => "routes",
            Self::Matrices => "matrices",
            Self::TravelModels => "travel-models",
            Self::Rasters => "rasters",
            Self::RasterDerivations => "raster-derivations",
            Self::SpatialDerivations => "spatial-derivations",
            Self::FeatureLayers => "feature-layers",
            Self::Publications => "publications",
            Self::LayerProducts => "layer-products",
            Self::Compositions => "compositions",
            Self::Workspace => "workspace",
        }
    }
}

/// Direct resource addresses require the identity types of every parent.
/// ```compile_fail
/// use veoveo_map_mcp::contract::{MapResource, FeatureLayerId};
/// MapResource::Composition { id: FeatureLayerId::new() };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub enum MapResource {
    Root(MapRoot),
    Document(MapDocument),
    WorkspaceApp,
    Acquisition {
        id: AcquisitionId,
    },
    Dataset {
        id: MapDatasetId,
    },
    Location {
        id: LocationId,
    },
    Facility {
        id: FacilityId,
    },
    Matrix {
        id: RouteMatrixId,
    },
    Layer {
        layer: FeatureLayerId,
    },
    Schema {
        layer: FeatureLayerId,
        version: u64,
    },
    Style {
        layer: FeatureLayerId,
        version: u64,
    },
    StyleRevision {
        id: StyleRevisionId,
    },
    Features {
        layer: FeatureLayerId,
    },
    Feature {
        layer: FeatureLayerId,
        feature: MapFeatureId,
    },
    FeatureRevision {
        layer: FeatureLayerId,
        feature: MapFeatureId,
        revision: u64,
    },
    Changeset {
        layer: FeatureLayerId,
        changeset: FeatureChangeSetId,
    },
    Publication {
        layer: FeatureLayerId,
        publication: LayerPublicationId,
    },
    Product {
        layer: FeatureLayerId,
        publication: LayerPublicationId,
        product: LayerProductId,
    },
    Composition {
        id: MapCompositionId,
    },
    CompositionRevision {
        id: MapCompositionId,
        revision: u64,
    },
}

impl MapResource {
    pub fn parse(value: &str) -> Result<Self, MapResourceError> {
        let parts = ResourceUriParts::parse(value).map_err(|_| MapResourceError)?;
        if parts.has_query() {
            return Err(MapResourceError);
        }
        let segments = parts.path_segments().collect::<Vec<_>>();
        let path = segments.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
        let address = match (parts.scheme(), parts.authority(), path.as_slice()) {
            ("ui", "map", ["workspace.html"]) => Self::WorkspaceApp,
            ("map", root, []) => Self::Root(MapRoot::parse(root)?),
            ("map", "docs", [document]) => Self::Document(MapDocument::parse(document)?),
            ("map", "acquisition", [id]) => Self::Acquisition {
                id: id.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "dataset", [id]) => Self::Dataset {
                id: id.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "location", [id]) => Self::Location {
                id: id.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "facility", [id]) => Self::Facility {
                id: id.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "matrix", [id]) => Self::Matrix {
                id: id.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-layer", [layer]) => Self::Layer {
                layer: layer.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-layer", [layer, "schema", version]) => Self::Schema {
                layer: layer.parse().map_err(|_| MapResourceError)?,
                version: version.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-layer", [layer, "style", version]) => Self::Style {
                layer: layer.parse().map_err(|_| MapResourceError)?,
                version: version.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-style", [id]) => Self::StyleRevision {
                id: id.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-layer", [layer, "features"]) => Self::Features {
                layer: layer.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-layer", [layer, "feature", feature]) => Self::Feature {
                layer: layer.parse().map_err(|_| MapResourceError)?,
                feature: feature.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-layer", [layer, "feature", feature, "revision", revision]) => {
                Self::FeatureRevision {
                    layer: layer.parse().map_err(|_| MapResourceError)?,
                    feature: feature.parse().map_err(|_| MapResourceError)?,
                    revision: revision.parse().map_err(|_| MapResourceError)?,
                }
            }
            ("map", "feature-layer", [layer, "changeset", changeset]) => Self::Changeset {
                layer: layer.parse().map_err(|_| MapResourceError)?,
                changeset: changeset.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-layer", [layer, "publication", publication]) => Self::Publication {
                layer: layer.parse().map_err(|_| MapResourceError)?,
                publication: publication.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "feature-layer", [layer, "publication", publication, "product", product]) => {
                Self::Product {
                    layer: layer.parse().map_err(|_| MapResourceError)?,
                    publication: publication.parse().map_err(|_| MapResourceError)?,
                    product: product.parse().map_err(|_| MapResourceError)?,
                }
            }
            ("map", "composition", [id]) => Self::Composition {
                id: id.parse().map_err(|_| MapResourceError)?,
            },
            ("map", "composition", [id, "revision", revision]) => Self::CompositionRevision {
                id: id.parse().map_err(|_| MapResourceError)?,
                revision: revision.parse().map_err(|_| MapResourceError)?,
            },
            _ => return Err(MapResourceError),
        };
        if address.to_uri().as_str() != value {
            return Err(MapResourceError);
        }
        Ok(address)
    }
    pub fn to_uri(&self) -> ResourceUri {
        let (root, segments): (&str, Vec<String>) = match self {
            Self::Root(root) => {
                return ResourceUriBuilder::from_components(
                    &crate::uris::SCHEME,
                    veoveo_types::UriAuthority::new(root.as_str()).expect("declared Map root"),
                )
                .expect("declared Map scheme")
                .build()
                .expect("declared Map root");
            }
            Self::Document(document) => ("map://docs", vec![document.as_str().to_owned()]),
            Self::WorkspaceApp => ("ui://map", vec!["workspace.html".to_owned()]),
            Self::Acquisition { id } => ("map://acquisition", vec![id.to_string()]),
            Self::Dataset { id } => ("map://dataset", vec![id.to_string()]),
            Self::Location { id } => ("map://location", vec![id.to_string()]),
            Self::Facility { id } => ("map://facility", vec![id.to_string()]),
            Self::Matrix { id } => ("map://matrix", vec![id.to_string()]),
            Self::Layer { layer } => ("map://feature-layer", vec![layer.to_string()]),
            Self::Schema { layer, version } => (
                "map://feature-layer",
                vec![layer.to_string(), "schema".to_owned(), version.to_string()],
            ),
            Self::Style { layer, version } => (
                "map://feature-layer",
                vec![layer.to_string(), "style".to_owned(), version.to_string()],
            ),
            Self::StyleRevision { id } => ("map://feature-style", vec![id.to_string()]),
            Self::Features { layer } => (
                "map://feature-layer",
                vec![layer.to_string(), "features".to_owned()],
            ),
            Self::Feature { layer, feature } => (
                "map://feature-layer",
                vec![layer.to_string(), "feature".to_owned(), feature.to_string()],
            ),
            Self::FeatureRevision {
                layer,
                feature,
                revision,
            } => (
                "map://feature-layer",
                vec![
                    layer.to_string(),
                    "feature".to_owned(),
                    feature.to_string(),
                    "revision".to_owned(),
                    revision.to_string(),
                ],
            ),
            Self::Changeset { layer, changeset } => (
                "map://feature-layer",
                vec![
                    layer.to_string(),
                    "changeset".to_owned(),
                    changeset.to_string(),
                ],
            ),
            Self::Publication { layer, publication } => (
                "map://feature-layer",
                vec![
                    layer.to_string(),
                    "publication".to_owned(),
                    publication.to_string(),
                ],
            ),
            Self::Product {
                layer,
                publication,
                product,
            } => (
                "map://feature-layer",
                vec![
                    layer.to_string(),
                    "publication".to_owned(),
                    publication.to_string(),
                    "product".to_owned(),
                    product.to_string(),
                ],
            ),
            Self::Composition { id } => ("map://composition", vec![id.to_string()]),
            Self::CompositionRevision { id, revision } => (
                "map://composition",
                vec![id.to_string(), "revision".to_owned(), revision.to_string()],
            ),
        };
        let mut builder = ResourceUriBuilder::new(root).expect("declared Map resource root");
        for segment in segments {
            builder =
                builder.segment(UriSegment::new(segment).expect("typed Map resource segment"));
        }
        builder.build().expect("typed Map resource address")
    }
}
impl ResourceAddress for MapResource {
    type Error = MapResourceError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.to_uri())
    }
}
impl TryFrom<String> for MapResource {
    type Error = MapResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<MapResource> for String {
    fn from(value: MapResource) -> Self {
        value.to_uri().to_string()
    }
}
