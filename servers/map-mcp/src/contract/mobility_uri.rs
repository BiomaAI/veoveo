//! Map-owned mobility-profile addresses, independent of the runtime and MCP.
use super::{MapMobilityProfileCursor, MobilityProfileId, MobilityProfileVersion};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MapMobilityError {
    #[error("invalid canonical Map mobility-profile address")]
    Address,
    #[error("invalid version 1 Map mobility-profile cursor")]
    Cursor,
    #[error("invalid Map mobility-profile page length, ordering or continuation")]
    Page,
    #[error("invalid Map mobility profile metadata or performance")]
    Metadata,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "map://mobility-profile/{profile_id}/{profile_version}", error = MapMobilityError, route_error = |_| MapMobilityError::Address, wire)]
pub struct MapMobilityProfileUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "profile_id", error = |_| MapMobilityError::Address)]
    id: MobilityProfileId,
    #[resource(variable = "profile_version", codec = MobilityVersionCodec, error = |_| MapMobilityError::Address)]
    version: MobilityProfileVersion,
}

impl MapMobilityProfileUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapMobilityProfileUri, RouteId};
    /// MapMobilityProfileUri::new(RouteId::new(), veoveo_map_mcp::contract::MobilityProfileVersion::FIRST);
    /// ```
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapMobilityProfileUri;
    /// MapMobilityProfileUri::new(veoveo_map_mcp::contract::MobilityProfileId::new(), 1);
    /// ```
    pub fn new(id: MobilityProfileId, version: MobilityProfileVersion) -> Self {
        Self::resource_from_parts(id, version).expect("typed Map mobility address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapMobilityError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapMobilityError::Address)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn version(&self) -> MobilityProfileVersion {
        self.version
    }
    pub fn id(&self) -> &MobilityProfileId {
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
#[resource(template = "map://mobility-profiles{?cursor}", error = MapMobilityError, route_error = |_| MapMobilityError::Address, wire)]
pub struct MapMobilityProfilesUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec = MapMobilityProfileCursorCodec, error = |error| error)]
    cursor: Option<MapMobilityProfileCursor>,
}
impl MapMobilityProfilesUri {
    pub const ROOT: &str = "map://mobility-profiles";
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(cursor: Option<MapMobilityProfileCursor>) -> Self {
        Self::resource_from_parts(cursor).expect("typed Map mobility address")
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapMobilityError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MapMobilityError::Address)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn cursor(&self) -> Option<&MapMobilityProfileCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

struct MapMobilityProfileCursorCodec;
impl ResourceFieldCodec<MapMobilityProfileCursor> for MapMobilityProfileCursorCodec {
    type Error = MapMobilityError;
    fn parse(value: &str) -> Result<MapMobilityProfileCursor, Self::Error> {
        MapMobilityProfileCursor::parse(value)
    }
    fn text(value: &MapMobilityProfileCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}

struct MobilityVersionCodec;
impl ResourceFieldCodec<MobilityProfileVersion> for MobilityVersionCodec {
    type Error = MapMobilityError;
    fn parse(value: &str) -> Result<MobilityProfileVersion, Self::Error> {
        value.parse().map_err(|_| MapMobilityError::Address)
    }
    fn text(value: &MobilityProfileVersion) -> std::borrow::Cow<'_, str> {
        value.to_string().into()
    }
}
