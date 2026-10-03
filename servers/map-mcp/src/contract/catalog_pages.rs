//! Typed selections and continuation positions for operational Map catalogs.
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

use super::{
    AcquisitionId, DatasetReleaseId, MapDatasetId, MapResourceError, RasterDerivationId, RouteId,
    RouteMatrixId, SpatialDerivationId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapCatalogPage {
    Routes {
        after: Option<RouteId>,
    },
    Matrices {
        after: Option<RouteMatrixId>,
    },
    Acquisitions {
        after: Option<AcquisitionId>,
    },
    Releases {
        dataset: Option<MapDatasetId>,
        after: Option<DatasetReleaseId>,
    },
    RasterDerivations {
        after: Option<RasterDerivationId>,
    },
    SpatialDerivations {
        after: Option<SpatialDerivationId>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "collection", rename_all = "snake_case", deny_unknown_fields)]
enum OwnedPosition {
    Routes { version: u8, after: RouteId },
    Matrices { version: u8, after: RouteMatrixId },
    Acquisitions { version: u8, after: AcquisitionId },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "collection", rename_all = "snake_case", deny_unknown_fields)]
enum ReleasePosition {
    Releases {
        version: u8,
        dataset: Option<MapDatasetId>,
        after: DatasetReleaseId,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DerivationPosition {
    Raster {
        version: u8,
        after: RasterDerivationId,
    },
    Spatial {
        version: u8,
        after: SpatialDerivationId,
    },
}

impl MapCatalogPage {
    pub fn parse(value: &str) -> Result<Self, MapResourceError> {
        let parts = ResourceUriParts::parse(value).map_err(|_| MapResourceError)?;
        if parts.scheme() != "map" {
            return Err(MapResourceError);
        }
        let path = parts.path_segments().collect::<Vec<_>>();
        let page = match (parts.authority(), path.as_slice()) {
            ("routes", []) => Self::Routes { after: None },
            ("matrices", []) => Self::Matrices { after: None },
            ("acquisitions", []) => Self::Acquisitions { after: None },
            ("datasets", []) => Self::Releases {
                dataset: None,
                after: None,
            },
            ("dataset", [dataset]) => Self::Releases {
                dataset: Some(dataset.parse().map_err(|_| MapResourceError)?),
                after: None,
            },
            ("raster-derivations", []) => Self::RasterDerivations { after: None },
            ("spatial-derivations", []) => Self::SpatialDerivations { after: None },
            _ => return Err(MapResourceError),
        };
        if path.iter().any(|p| matches!(p, std::borrow::Cow::Owned(_))) {
            return Err(MapResourceError);
        }
        let parameters = parts.query_parameters();
        let cursor = if !parts.has_query() {
            None
        } else if parameters.len() == 1 {
            Some(parameters.get("cursor").ok_or(MapResourceError)?.as_str())
        } else {
            return Err(MapResourceError);
        };
        page.resume(cursor)
    }

    /// Decode only a cursor for this collection and selected dataset. Positions
    /// keep the owning ID type until the Store adapter binds its database key.
    pub fn resume(self, cursor: Option<&str>) -> Result<Self, MapResourceError> {
        let Some(cursor) = cursor else {
            return Ok(self);
        };
        if cursor.is_empty() || cursor.len() > 2048 {
            return Err(MapResourceError);
        }
        let bytes = hex::decode(cursor).map_err(|_| MapResourceError)?;
        match self {
            Self::Routes { .. } | Self::Matrices { .. } | Self::Acquisitions { .. } => {
                let position: OwnedPosition =
                    serde_json::from_slice(&bytes).map_err(|_| MapResourceError)?;
                match (self, position) {
                    (Self::Routes { .. }, OwnedPosition::Routes { version: 1, after }) => {
                        Ok(Self::Routes { after: Some(after) })
                    }
                    (Self::Matrices { .. }, OwnedPosition::Matrices { version: 1, after }) => {
                        Ok(Self::Matrices { after: Some(after) })
                    }
                    (
                        Self::Acquisitions { .. },
                        OwnedPosition::Acquisitions { version: 1, after },
                    ) => Ok(Self::Acquisitions { after: Some(after) }),
                    _ => Err(MapResourceError),
                }
            }
            Self::Releases { dataset, .. } => {
                let position: ReleasePosition =
                    serde_json::from_slice(&bytes).map_err(|_| MapResourceError)?;
                match position {
                    ReleasePosition::Releases {
                        version: 1,
                        dataset: parent,
                        after,
                    } if dataset == parent => Ok(Self::Releases {
                        dataset,
                        after: Some(after),
                    }),
                    _ => Err(MapResourceError),
                }
            }
            Self::RasterDerivations { .. } | Self::SpatialDerivations { .. } => {
                let position: DerivationPosition =
                    serde_json::from_slice(&bytes).map_err(|_| MapResourceError)?;
                match (self, position) {
                    (
                        Self::RasterDerivations { .. },
                        DerivationPosition::Raster { version: 1, after },
                    ) => Ok(Self::RasterDerivations { after: Some(after) }),
                    (
                        Self::SpatialDerivations { .. },
                        DerivationPosition::Spatial { version: 1, after },
                    ) => Ok(Self::SpatialDerivations { after: Some(after) }),
                    _ => Err(MapResourceError),
                }
            }
        }
    }

    pub fn cursor(&self) -> Option<String> {
        fn encode(value: impl Serialize) -> String {
            hex::encode(serde_json::to_vec(&value).expect("typed Map catalog position"))
        }
        match self {
            Self::Routes { after } => after
                .clone()
                .map(|after| encode(OwnedPosition::Routes { version: 1, after })),
            Self::Matrices { after } => after
                .clone()
                .map(|after| encode(OwnedPosition::Matrices { version: 1, after })),
            Self::Acquisitions { after } => after
                .clone()
                .map(|after| encode(OwnedPosition::Acquisitions { version: 1, after })),
            Self::Releases { dataset, after } => after.clone().map(|after| {
                encode(ReleasePosition::Releases {
                    version: 1,
                    dataset: dataset.clone(),
                    after,
                })
            }),
            Self::RasterDerivations { after } => after
                .clone()
                .map(|after| encode(DerivationPosition::Raster { version: 1, after })),
            Self::SpatialDerivations { after } => after
                .clone()
                .map(|after| encode(DerivationPosition::Spatial { version: 1, after })),
        }
    }
}
impl ResourceAddress for MapCatalogPage {
    type Error = MapResourceError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let root = match self {
            Self::Routes { .. } => "map://routes",
            Self::Matrices { .. } => "map://matrices",
            Self::Acquisitions { .. } => "map://acquisitions",
            Self::Releases { dataset: None, .. } => "map://datasets",
            Self::Releases {
                dataset: Some(_), ..
            } => "map://dataset",
            Self::RasterDerivations { .. } => "map://raster-derivations",
            Self::SpatialDerivations { .. } => "map://spatial-derivations",
        };
        let mut builder = ResourceUriBuilder::new(root).map_err(|_| MapResourceError)?;
        if let Self::Releases {
            dataset: Some(dataset),
            ..
        } = self
        {
            builder =
                builder.segment(UriSegment::new(dataset.as_str()).map_err(|_| MapResourceError)?);
        }
        if let Some(cursor) = self.cursor() {
            builder = builder
                .query_pair("cursor", &cursor)
                .map_err(|_| MapResourceError)?;
        }
        builder.build().map_err(|_| MapResourceError)
    }
}
