//! Resource addresses and opaque continuation for authoring metadata collections.
use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts};

use super::{FeatureLayerId, LayerProductId, LayerPublicationId, MapCompositionId};
use crate::uris;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapMetadataError {
    UnknownResource,
    InvalidAddress,
    InvalidSelection,
    InvalidCursor,
}

impl fmt::Display for MapMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownResource => "unknown Map metadata resource",
            Self::InvalidAddress => "invalid Map metadata resource address",
            Self::InvalidSelection => "invalid Map metadata selection or parameters",
            Self::InvalidCursor => "invalid Map metadata cursor or mismatched selection",
        })
    }
}
impl Error for MapMetadataError {}

/// Each collection preserves its own ID type and optional parent selection.
/// ```compile_fail
/// use veoveo_map_mcp::contract::{MapMetadataRequest, LayerPublicationId};
/// MapMetadataRequest::Layers { after: Some(LayerPublicationId::new()) };
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MapMetadataRequest {
    Layers {
        after: Option<FeatureLayerId>,
    },
    Publications {
        layer: Option<FeatureLayerId>,
        after: Option<LayerPublicationId>,
    },
    Products {
        publication: Option<LayerPublicationId>,
        after: Option<LayerProductId>,
    },
    Compositions {
        after: Option<MapCompositionId>,
    },
}

impl MapMetadataRequest {
    pub fn parse(value: &str) -> Result<Self, MapMetadataError> {
        let parts = ResourceUriParts::parse(value).map_err(|_| MapMetadataError::InvalidAddress)?;
        if parts.scheme() != "map" || parts.path_segments().next().is_some() {
            return Err(MapMetadataError::UnknownResource);
        }
        let mut selection = match parts.authority() {
            "feature-layers" => Self::Layers { after: None },
            "publications" => Self::Publications {
                layer: None,
                after: None,
            },
            "layer-products" => Self::Products {
                publication: None,
                after: None,
            },
            "compositions" => Self::Compositions { after: None },
            _ => return Err(MapMetadataError::UnknownResource),
        };
        if value.len() > 4096 {
            return Err(MapMetadataError::InvalidAddress);
        }
        let mut cursor = None;
        for (name, value) in parts.query_parameters() {
            match (name.as_str(), &mut selection) {
                ("cursor", _) => cursor = Some(MapMetadataCursor::parse(value)?),
                ("layer_id", Self::Publications { layer, .. }) => {
                    *layer = Some(
                        value
                            .parse()
                            .map_err(|_| MapMetadataError::InvalidSelection)?,
                    );
                }
                ("publication_id", Self::Products { publication, .. }) => {
                    *publication = Some(
                        value
                            .parse()
                            .map_err(|_| MapMetadataError::InvalidSelection)?,
                    );
                }
                _ => return Err(MapMetadataError::InvalidSelection),
            }
        }
        if parts.has_query() && parts.query_parameters().is_empty() {
            return Err(MapMetadataError::InvalidSelection);
        }
        match cursor {
            Some(cursor) => cursor.resume(&selection),
            None => Ok(selection),
        }
    }

    fn same_selection(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Layers { .. }, Self::Layers { .. })
            | (Self::Compositions { .. }, Self::Compositions { .. }) => true,
            (Self::Publications { layer, .. }, Self::Publications { layer: parent, .. }) => {
                layer == parent
            }
            (
                Self::Products { publication, .. },
                Self::Products {
                    publication: parent,
                    ..
                },
            ) => publication == parent,
            _ => false,
        }
    }

    fn has_position(&self) -> bool {
        match self {
            Self::Layers { after } => after.is_some(),
            Self::Publications { after, .. } => after.is_some(),
            Self::Products { after, .. } => after.is_some(),
            Self::Compositions { after } => after.is_some(),
        }
    }

    /// Produce a continuation only when a typed position is present.
    pub fn cursor(&self) -> Option<MapMetadataCursor> {
        self.has_position()
            .then(|| MapMetadataCursor::from_request(self.clone()))
    }
}

impl ResourceAddress for MapMetadataRequest {
    type Error = MapMetadataError;

    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }

    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let (root, parent) = match self {
            Self::Layers { .. } => (uris::FEATURE_LAYERS_URI, None),
            Self::Publications { layer, .. } => (
                uris::PUBLICATIONS_URI,
                layer.as_ref().map(|id| ("layer_id", id.as_str())),
            ),
            Self::Products { publication, .. } => (
                uris::LAYER_PRODUCTS_URI,
                publication
                    .as_ref()
                    .map(|id| ("publication_id", id.as_str())),
            ),
            Self::Compositions { .. } => (uris::COMPOSITIONS_URI, None),
        };
        let build = || {
            let mut builder = ResourceUriBuilder::new(root)?;
            if let Some((name, value)) = parent {
                builder = builder.query_pair(name, value)?;
            }
            if let Some(cursor) = self.cursor() {
                builder = builder.query_pair("cursor", cursor.as_str())?;
            }
            builder.build()
        };
        build().map_err(|_: veoveo_types::ResourceUriError| MapMetadataError::InvalidAddress)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorEnvelope {
    version: u8,
    request: MapMetadataRequest,
}

/// The cursor binds a typed position to its collection and optional parent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MapMetadataCursor {
    cursor: veoveo_types::OpaqueCursor<MapMetadataCursorCodec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MapMetadataCursorCodec;
impl veoveo_types::CursorCodec for MapMetadataCursorCodec {
    type Position = MapMetadataRequest;
    type Error = MapMetadataError;
    fn check(&self, request: &MapMetadataRequest) -> Result<(), Self::Error> {
        if request.has_position() {
            Ok(())
        } else {
            Err(MapMetadataError::InvalidCursor)
        }
    }
    fn encode(&self, request: &MapMetadataRequest) -> Result<String, Self::Error> {
        Ok(hex::encode(
            serde_json::to_vec(&CursorEnvelope {
                version: 1,
                request: request.clone(),
            })
            .expect("closed Map cursor fields serialize"),
        ))
    }
    fn decode(&self, wire: &str) -> Result<MapMetadataRequest, Self::Error> {
        if wire.is_empty() || wire.len() > 2048 {
            return Err(MapMetadataError::InvalidCursor);
        }
        let bytes = hex::decode(wire).map_err(|_| MapMetadataError::InvalidCursor)?;
        let envelope: CursorEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| MapMetadataError::InvalidCursor)?;
        if envelope.version != 1 {
            return Err(MapMetadataError::InvalidCursor);
        }
        Ok(envelope.request)
    }
}
impl MapMetadataCursor {
    fn from_request(request: MapMetadataRequest) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(MapMetadataCursorCodec, request)
                .expect("positioned Map request"),
        }
    }

    pub fn parse(wire: impl Into<String>) -> Result<Self, MapMetadataError> {
        veoveo_types::OpaqueCursor::parse(MapMetadataCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }

    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }

    pub fn resume(
        &self,
        selection: &MapMetadataRequest,
    ) -> Result<MapMetadataRequest, MapMetadataError> {
        if !self.cursor.position().same_selection(selection) {
            return Err(MapMetadataError::InvalidCursor);
        }
        Ok(self.cursor.position().clone())
    }
}

impl TryFrom<String> for MapMetadataCursor {
    type Error = MapMetadataError;
    fn try_from(wire: String) -> Result<Self, Self::Error> {
        Self::parse(wire)
    }
}
impl From<MapMetadataCursor> for String {
    fn from(cursor: MapMetadataCursor) -> Self {
        cursor.cursor.into_wire()
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MapMetadataPage<T> {
    pub items: Vec<T>,
    pub limit: usize,
    pub next_cursor: Option<MapMetadataCursor>,
}
