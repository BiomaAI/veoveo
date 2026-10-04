//! Map-owned source addresses, independent of the runtime and MCP.
use super::{MapSourceCursor, MapSourceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MapSourceError {
    #[error("invalid canonical Map source address")]
    Address,
    #[error("invalid version 1 Map source cursor")]
    Cursor,
    #[error("invalid Map source page length, ordering or continuation")]
    Page,
    #[error("invalid Map source summary identity, validity or metadata")]
    Metadata,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://source/{source_id}", error = MapSourceError, route_error = |_| MapSourceError::Address, wire)]
pub struct MapSourceUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "source_id", error = |_| MapSourceError::Address)]
    id: MapSourceId,
}

impl MapSourceUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapSourceUri, RouteId};
    /// MapSourceUri::new(RouteId::new());
    /// ```
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapSourceUri;
    /// MapSourceUri::new("source-id");
    /// ```
    pub fn new(id: MapSourceId) -> Self {
        Self::resource_from_parts(id).expect("typed Map source address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapSourceError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapSourceError::Address)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &MapSourceId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://sources{?cursor}", error = MapSourceError, route_error = |_| MapSourceError::Address, wire)]
pub struct MapSourcesUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec = MapSourceCursorCodec, error = |error| error)]
    cursor: Option<MapSourceCursor>,
}
impl MapSourcesUri {
    pub const ROOT: &str = "map://sources";
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(cursor: Option<MapSourceCursor>) -> Self {
        Self::resource_from_parts(cursor).expect("typed Map source address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapSourceError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapSourceError::Address)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn cursor(&self) -> Option<&MapSourceCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

struct MapSourceCursorCodec;
impl ResourceFieldCodec<MapSourceCursor> for MapSourceCursorCodec {
    type Error = MapSourceError;
    fn parse(value: &str) -> Result<MapSourceCursor, Self::Error> {
        MapSourceCursor::parse(value)
    }
    fn text(value: &MapSourceCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
