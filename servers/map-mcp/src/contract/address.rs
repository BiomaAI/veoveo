//! The host admits every public Map family once, before typed domain dispatch.
use super::*;
use veoveo_types::{ResourceAddress, ResourceUri, ResourceUriParts};

/// A validated hosted address. Construction selects the owning resource parser;
/// callers cannot construct a target that disagrees with the retained wire URI.
#[derive(Debug, Clone)]
pub struct MapAddress {
    uri: ResourceUri,
    target: MapTarget,
}

#[derive(Debug, Clone)]
pub enum MapTarget {
    Resource(MapResource),
    Catalog(MapCatalogPage),
    Metadata(MapMetadataRequest),
    Features(QueryFeaturesRequest),
    Dataset(MapDatasetAddress),
    Artifact(veoveo_artifact_contract::ArtifactId),
    KnowledgePage(MapKnowledgePageUri),
    KnowledgeMember(MapKnowledgeMember),
}

#[derive(Debug, Clone)]
pub enum MapDatasetAddress {
    Sources(MapSourcesUri),
    Source(MapSourceUri),
    MobilityProfiles(MapMobilityProfilesUri),
    MobilityProfile(MapMobilityProfileUri),
    Restrictions(MapRestrictionsUri),
    Restriction(MapRestrictionUri),
    TravelModels(MapTravelModelsUri),
    TravelModel(MapTravelModelUri),
    Release(MapReleaseUri),
    SourceFeature(MapSourceFeatureUri),
    Raster(MapRasterUri),
    RasterDerivation(MapRasterDerivationUri),
    SpatialDerivation(MapSpatialDerivationUri),
    Route(MapRouteUri),
}

impl MapAddress {
    pub fn parse(value: &str) -> Result<Self, MapResourceError> {
        let uri = ResourceUri::new(value).map_err(|_| MapResourceError)?;
        let parts = ResourceUriParts::parse(value).map_err(|_| MapResourceError)?;
        let target = if let Ok(page) = MapKnowledgePageUri::parse(value) {
            MapTarget::KnowledgePage(page)
        } else if let Ok(member) = MapKnowledgeMember::parse(value) {
            MapTarget::KnowledgeMember(member)
        } else if parts.scheme() == "map" {
            match parts.authority() {
                "sources" => MapTarget::Dataset(MapDatasetAddress::Sources(
                    MapSourcesUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "source" => MapTarget::Dataset(MapDatasetAddress::Source(
                    MapSourceUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "mobility-profiles" => MapTarget::Dataset(MapDatasetAddress::MobilityProfiles(
                    MapMobilityProfilesUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "mobility-profile" => MapTarget::Dataset(MapDatasetAddress::MobilityProfile(
                    MapMobilityProfileUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "restrictions" => MapTarget::Dataset(MapDatasetAddress::Restrictions(
                    MapRestrictionsUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "restriction" => MapTarget::Dataset(MapDatasetAddress::Restriction(
                    MapRestrictionUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "travel-models" => MapTarget::Dataset(MapDatasetAddress::TravelModels(
                    MapTravelModelsUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "travel-model" => MapTarget::Dataset(MapDatasetAddress::TravelModel(
                    MapTravelModelUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "source-feature" => MapTarget::Dataset(MapDatasetAddress::SourceFeature(
                    MapSourceFeatureUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "raster" => MapTarget::Dataset(MapDatasetAddress::Raster(
                    MapRasterUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "raster-derivation" => MapTarget::Dataset(MapDatasetAddress::RasterDerivation(
                    MapRasterDerivationUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "spatial-derivation" => MapTarget::Dataset(MapDatasetAddress::SpatialDerivation(
                    MapSpatialDerivationUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "route" => MapTarget::Dataset(MapDatasetAddress::Route(
                    MapRouteUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                "artifact" => {
                    MapTarget::Artifact(crate::uris::parse_artifact(value).ok_or(MapResourceError)?)
                }
                "feature-layers" | "publications" | "layer-products" | "compositions" => {
                    MapTarget::Metadata(
                        MapMetadataRequest::parse(value).map_err(|_| MapResourceError)?,
                    )
                }
                "routes"
                | "matrices"
                | "acquisitions"
                | "datasets"
                | "raster-derivations"
                | "spatial-derivations" => MapTarget::Catalog(MapCatalogPage::parse(value)?),
                "dataset" if parts.path_segments().count() == 1 => {
                    MapTarget::Catalog(MapCatalogPage::parse(value)?)
                }
                "dataset" => MapTarget::Dataset(MapDatasetAddress::Release(
                    MapReleaseUri::parse(value).map_err(|_| MapResourceError)?,
                )),
                _ => match crate::uris::parse_features_request(value)
                    .map_err(|_| MapResourceError)?
                {
                    Some(request) => MapTarget::Features(request),
                    None => MapTarget::Resource(MapResource::parse(value)?),
                },
            }
        } else {
            MapTarget::Resource(MapResource::parse(value)?)
        };
        Ok(Self { uri, target })
    }
    pub fn target(&self) -> &MapTarget {
        &self.target
    }

    pub fn into_target(self) -> MapTarget {
        self.target
    }

    pub fn to_uri(&self) -> ResourceUri {
        self.uri.clone()
    }
}
impl PartialEq for MapAddress {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri
    }
}
impl Eq for MapAddress {}
impl ResourceAddress for MapAddress {
    type Error = MapResourceError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.to_uri())
    }
}
