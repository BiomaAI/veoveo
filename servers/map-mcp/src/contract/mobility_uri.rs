//! Map-owned mobility-profile addresses, independent of the runtime and MCP.
use super::{MapMobilityProfileCursor, MobilityProfileId, MobilityProfileVersion};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapMobilityProfileUri {
    wire: ResourceUri,
    id: MobilityProfileId,
    version: MobilityProfileVersion,
}

impl MapMobilityProfileUri {
    pub const TEMPLATE: &str = "map://mobility-profile/{profile_id}/{profile_version}";

    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapMobilityProfileUri, RouteId};
    /// MapMobilityProfileUri::new(RouteId::new(), veoveo_map_mcp::contract::MobilityProfileVersion::FIRST);
    /// ```
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapMobilityProfileUri;
    /// MapMobilityProfileUri::new(veoveo_map_mcp::contract::MobilityProfileId::new(), 1);
    /// ```
    pub fn new(id: MobilityProfileId, version: MobilityProfileVersion) -> Self {
        let wire = ResourceUriBuilder::new("map://mobility-profile")
            .expect("declared Map root")
            .segment(UriSegment::new(id.to_string()).expect("typed mobility-profile ID"))
            .segment(UriSegment::new(version.to_string()).expect("typed profile version"))
            .build()
            .expect("typed mobility-profile address");
        Self { wire, id, version }
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapMobilityError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MapMobilityError::Address)?;
        let path: Vec<_> = parts.path_segments().collect();
        if parts.scheme() != "map"
            || parts.authority() != "mobility-profile"
            || parts.has_query()
            || path.len() != 2
        {
            return Err(MapMobilityError::Address);
        }
        let address = Self::new(
            MobilityProfileId::parse(path[0].as_ref()).map_err(|_| MapMobilityError::Address)?,
            path[1].parse().map_err(|_| MapMobilityError::Address)?,
        );
        if address.as_str() != value {
            return Err(MapMobilityError::Address);
        }
        Ok(address)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapMobilityProfilesUri {
    wire: ResourceUri,
    cursor: Option<MapMobilityProfileCursor>,
}
impl MapMobilityProfilesUri {
    pub const ROOT: &str = "map://mobility-profiles";
    pub const TEMPLATE: &str = "map://mobility-profiles{?cursor}";

    pub fn new(cursor: Option<MapMobilityProfileCursor>) -> Self {
        let mut builder = ResourceUriBuilder::new(Self::ROOT).expect("declared Map root");
        if let Some(cursor) = &cursor {
            builder = builder
                .query_pair("cursor", cursor.as_str())
                .expect("typed cursor");
        }
        Self {
            wire: builder.build().expect("typed mobility-profile collection"),
            cursor,
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapMobilityError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MapMobilityError::Address)?;
        if parts.scheme() != "map"
            || parts.authority() != "mobility-profiles"
            || parts.path_segments().next().is_some()
        {
            return Err(MapMobilityError::Address);
        }
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(MapMobilityError::Address);
            }
            Some(MapMobilityProfileCursor::parse(
                query
                    .get("cursor")
                    .ok_or(MapMobilityError::Address)?
                    .clone(),
            )?)
        } else {
            None
        };
        let address = Self::new(cursor);
        if address.as_str() != value {
            return Err(MapMobilityError::Address);
        }
        Ok(address)
    }
    pub fn cursor(&self) -> Option<&MapMobilityProfileCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

macro_rules! address_wire {
    ($ty:ty) => {
        impl TryFrom<String> for $ty {
            type Error = MapMobilityError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }
        impl From<$ty> for String {
            fn from(value: $ty) -> Self {
                value.wire.to_string()
            }
        }
        impl ResourceAddress for $ty {
            type Error = MapMobilityError;
            fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(value.as_str())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok(self.wire.clone())
            }
        }
    };
}
address_wire!(MapMobilityProfileUri);
address_wire!(MapMobilityProfilesUri);
