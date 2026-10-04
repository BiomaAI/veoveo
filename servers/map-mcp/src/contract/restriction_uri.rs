//! Map-owned restriction addresses, independent of the runtime and MCP.
use super::{MapRestrictionCursor, RestrictionId};

use veoveo_types::{ResourceFieldCodec, ResourceUri};

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

#[veoveo_types::resource_address(cached(MapRestrictionErrorAddresses), template = "map://restriction/{restriction_id}", schema = string, no_display)]
pub struct MapRestrictionUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "restriction_id", error = |_| MapRestrictionError::Address, accessor = id)]
    id: RestrictionId,
}

impl MapRestrictionUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached(MapRestrictionErrorAddresses), template = "map://restrictions{?cursor}", schema = string, no_display)]
pub struct MapRestrictionsUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec = MapRestrictionCursorCodec,  accessor = cursor)]
    cursor: Option<MapRestrictionCursor>,
}
impl MapRestrictionsUri {
    pub const ROOT: &str = "map://restrictions";
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
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

#[doc(hidden)]
pub struct MapRestrictionErrorAddresses;
impl veoveo_types::ResourceProfile for MapRestrictionErrorAddresses {
    type Error = MapRestrictionError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| MapRestrictionError::Address,
        };
}

/// ```compile_fail
/// use veoveo_map_mcp::contract::{MapRestrictionUri, RouteId};
/// MapRestrictionUri::new(RouteId::new());
/// ```
/// ```compile_fail
/// use veoveo_map_mcp::contract::MapRestrictionUri;
/// MapRestrictionUri::new("restriction-id");
/// ```
const _: () = ();
