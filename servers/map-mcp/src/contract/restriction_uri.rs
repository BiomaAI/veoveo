//! Map-owned restriction addresses, independent of the runtime and MCP.
use super::{MapRestrictionCursor, RestrictionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MapRestrictionError {
    #[error("invalid canonical Map restriction address")]
    Address,
    #[error("invalid version 1 Map restriction cursor")]
    Cursor,
    #[error("invalid Map restriction page length, ordering or continuation")]
    Page,
    #[error("invalid Map restriction summary identity, validity or metadata")]
    Metadata,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://restriction/{restriction_id}", error = MapRestrictionError, route_error = |_| MapRestrictionError::Address, wire)]
pub struct MapRestrictionUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "restriction_id", error = |_| MapRestrictionError::Address)]
    id: RestrictionId,
}

impl MapRestrictionUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapRestrictionUri, RouteId};
    /// MapRestrictionUri::new(RouteId::new());
    /// ```
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapRestrictionUri;
    /// MapRestrictionUri::new("restriction-id");
    /// ```
    pub fn new(id: RestrictionId) -> Self {
        Self::resource_from_parts(id).expect("typed Map restriction address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapRestrictionError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapRestrictionError::Address)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &RestrictionId {
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
#[resource(template = "map://restrictions{?cursor}", error = MapRestrictionError, route_error = |_| MapRestrictionError::Address, wire)]
pub struct MapRestrictionsUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec = MapRestrictionCursorCodec, error = |error| error)]
    cursor: Option<MapRestrictionCursor>,
}
impl MapRestrictionsUri {
    pub const ROOT: &str = "map://restrictions";
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(cursor: Option<MapRestrictionCursor>) -> Self {
        Self::resource_from_parts(cursor).expect("typed Map restriction address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapRestrictionError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapRestrictionError::Address)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn cursor(&self) -> Option<&MapRestrictionCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

struct MapRestrictionCursorCodec;
impl ResourceFieldCodec<MapRestrictionCursor> for MapRestrictionCursorCodec {
    type Error = MapRestrictionError;
    fn parse(value: &str) -> Result<MapRestrictionCursor, Self::Error> {
        MapRestrictionCursor::parse(value)
    }
    fn text(value: &MapRestrictionCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
