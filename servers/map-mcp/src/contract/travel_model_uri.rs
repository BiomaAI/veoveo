//! Map-owned travel-model identity shared with consumers through the contract feature.
use super::TravelModelId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected a canonical Map travel-model address or version 1 native Task cursor")]
pub struct TravelModelUriError;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapTravelModelUri {
    wire: String,
    id: TravelModelId,
}

impl MapTravelModelUri {
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapTravelModelUri, RouteId};
    /// MapTravelModelUri::new(RouteId::new());
    /// ```
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapTravelModelUri;
    /// MapTravelModelUri::new("travel-model-id");
    /// ```
    pub fn new(id: TravelModelId) -> Self {
        let wire = ResourceUriBuilder::new("map://travel-model")
            .expect("declared Map root")
            .segment(UriSegment::new(id.to_string()).expect("typed travel-model identity"))
            .build()
            .expect("typed travel-model address")
            .to_string();
        Self { wire, id }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TravelModelUriError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| TravelModelUriError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        if parts.scheme() != "map"
            || parts.authority() != "travel-model"
            || parts.has_query()
            || path.len() != 1
        {
            return Err(TravelModelUriError);
        }
        let address =
            Self::new(TravelModelId::parse(path[0].as_ref()).map_err(|_| TravelModelUriError)?);
        if address.as_str() != value {
            return Err(TravelModelUriError);
        }
        Ok(address)
    }
    pub fn id(&self) -> &TravelModelId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}
impl fmt::Display for MapTravelModelUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire)
    }
}
impl TryFrom<String> for MapTravelModelUri {
    type Error = TravelModelUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MapTravelModelUri> for String {
    fn from(value: MapTravelModelUri) -> Self {
        value.wire
    }
}
impl ResourceAddress for MapTravelModelUri {
    type Error = TravelModelUriError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        ResourceUri::new(&self.wire).map_err(|_| TravelModelUriError)
    }
}
