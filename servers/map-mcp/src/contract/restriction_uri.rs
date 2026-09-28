//! Map-owned restriction addresses, independent of the runtime and MCP.
use super::{MapRestrictionCursor, RestrictionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapRestrictionUri {
    wire: ResourceUri,
    id: RestrictionId,
}

impl MapRestrictionUri {
    pub const TEMPLATE: &str = "map://restriction/{restriction_id}";

    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapRestrictionUri, RouteId};
    /// MapRestrictionUri::new(RouteId::new());
    /// ```
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapRestrictionUri;
    /// MapRestrictionUri::new("restriction-id");
    /// ```
    pub fn new(id: RestrictionId) -> Self {
        let wire = ResourceUriBuilder::new("map://restriction")
            .expect("declared Map root")
            .segment(UriSegment::new(id.to_string()).expect("typed restriction ID"))
            .build()
            .expect("typed restriction address");
        Self { wire, id }
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapRestrictionError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MapRestrictionError::Address)?;
        let path: Vec<_> = parts.path_segments().collect();
        if parts.scheme() != "map"
            || parts.authority() != "restriction"
            || parts.has_query()
            || path.len() != 1
        {
            return Err(MapRestrictionError::Address);
        }
        let address = Self::new(
            RestrictionId::parse(path[0].as_ref()).map_err(|_| MapRestrictionError::Address)?,
        );
        if address.as_str() != value {
            return Err(MapRestrictionError::Address);
        }
        Ok(address)
    }
    pub fn id(&self) -> &RestrictionId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapRestrictionsUri {
    wire: ResourceUri,
    cursor: Option<MapRestrictionCursor>,
}
impl MapRestrictionsUri {
    pub const ROOT: &str = "map://restrictions";
    pub const TEMPLATE: &str = "map://restrictions{?cursor}";

    pub fn new(cursor: Option<MapRestrictionCursor>) -> Self {
        let mut builder = ResourceUriBuilder::new(Self::ROOT).expect("declared Map root");
        if let Some(cursor) = &cursor {
            builder = builder
                .query_pair("cursor", cursor.as_str())
                .expect("typed cursor");
        }
        Self {
            wire: builder.build().expect("typed restriction collection"),
            cursor,
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapRestrictionError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MapRestrictionError::Address)?;
        if parts.scheme() != "map"
            || parts.authority() != "restrictions"
            || parts.path_segments().next().is_some()
        {
            return Err(MapRestrictionError::Address);
        }
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(MapRestrictionError::Address);
            }
            Some(MapRestrictionCursor::parse(
                query
                    .get("cursor")
                    .ok_or(MapRestrictionError::Address)?
                    .clone(),
            )?)
        } else {
            None
        };
        let address = Self::new(cursor);
        if address.as_str() != value {
            return Err(MapRestrictionError::Address);
        }
        Ok(address)
    }
    pub fn cursor(&self) -> Option<&MapRestrictionCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

macro_rules! address_wire {
    ($ty:ty) => {
        impl TryFrom<String> for $ty {
            type Error = MapRestrictionError;
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
            type Error = MapRestrictionError;
            fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(value.as_str())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok(self.wire.clone())
            }
        }
    };
}
address_wire!(MapRestrictionUri);
address_wire!(MapRestrictionsUri);
