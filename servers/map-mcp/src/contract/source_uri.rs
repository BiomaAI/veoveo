//! Map-owned source addresses, independent of the runtime and MCP.
use super::{MapSourceCursor, MapSourceId};

use veoveo_types::{ResourceFieldCodec, ResourceUri};

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

#[veoveo_types::resource_address(cached(MapSourceErrorAddresses), template = "map://source/{source_id}", schema = string, no_display)]
pub struct MapSourceUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "source_id", error = |_| MapSourceError::Address, accessor = id)]
    id: MapSourceId,
}

impl MapSourceUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached(MapSourceErrorAddresses), template = "map://sources{?cursor}", schema = string, no_display)]
pub struct MapSourcesUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec = MapSourceCursorCodec,  accessor = cursor)]
    cursor: Option<MapSourceCursor>,
}
impl MapSourcesUri {
    pub const ROOT: &str = "map://sources";
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
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

#[doc(hidden)]
pub struct MapSourceErrorAddresses;
impl veoveo_types::ResourceProfile for MapSourceErrorAddresses {
    type Error = MapSourceError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| MapSourceError::Address,
        };
}

/// ```compile_fail
/// use veoveo_map_mcp::contract::{MapSourceUri, RouteId};
/// MapSourceUri::new(RouteId::new());
/// ```
/// ```compile_fail
/// use veoveo_map_mcp::contract::MapSourceUri;
/// MapSourceUri::new("source-id");
/// ```
const _: () = ();
