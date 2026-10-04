//! Map-owned mobility-profile addresses, independent of the runtime and MCP.
use super::{MapMobilityProfileCursor, MobilityProfileId, MobilityProfileVersion};

use veoveo_types::{ResourceFieldCodec, ResourceUri};

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

#[veoveo_types::resource_address(cached(MapMobilityErrorAddresses), template = "map://mobility-profile/{profile_id}/{profile_version}", schema = string, no_display)]
pub struct MapMobilityProfileUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "profile_id", error = |_| MapMobilityError::Address, accessor = id)]
    id: MobilityProfileId,
    #[resource(variable = "profile_version", codec = MobilityVersionCodec, error = |_| MapMobilityError::Address, accessor = version, copy_accessor)]
    version: MobilityProfileVersion,
}

impl MapMobilityProfileUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached(MapMobilityErrorAddresses), template = "map://mobility-profiles{?cursor}", schema = string, no_display)]
pub struct MapMobilityProfilesUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec = MapMobilityProfileCursorCodec,  accessor = cursor)]
    cursor: Option<MapMobilityProfileCursor>,
}
impl MapMobilityProfilesUri {
    pub const ROOT: &str = "map://mobility-profiles";
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
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

#[doc(hidden)]
pub struct MapMobilityErrorAddresses;
impl veoveo_types::ResourceProfile for MapMobilityErrorAddresses {
    type Error = MapMobilityError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| MapMobilityError::Address,
        };
}

/// ```compile_fail
/// use veoveo_map_mcp::contract::{MapMobilityProfileUri, RouteId};
/// MapMobilityProfileUri::new(RouteId::new(), veoveo_map_mcp::contract::MobilityProfileVersion::FIRST);
/// ```
/// ```compile_fail
/// use veoveo_map_mcp::contract::MapMobilityProfileUri;
/// MapMobilityProfileUri::new(veoveo_map_mcp::contract::MobilityProfileId::new(), 1);
/// ```
const _: () = ();
