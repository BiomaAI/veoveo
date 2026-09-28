//! Map-owned source addresses, independent of the runtime and MCP.
use super::{MapSourceCursor, MapSourceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapSourceUri {
    wire: ResourceUri,
    id: MapSourceId,
}

impl MapSourceUri {
    pub const TEMPLATE: &str = "map://source/{source_id}";

    /// ```compile_fail
    /// use veoveo_map_mcp::contract::{MapSourceUri, RouteId};
    /// MapSourceUri::new(RouteId::new());
    /// ```
    /// ```compile_fail
    /// use veoveo_map_mcp::contract::MapSourceUri;
    /// MapSourceUri::new("source-id");
    /// ```
    pub fn new(id: MapSourceId) -> Self {
        let wire = ResourceUriBuilder::new("map://source")
            .expect("declared Map root")
            .segment(UriSegment::new(id.to_string()).expect("typed source ID"))
            .build()
            .expect("typed source address");
        Self { wire, id }
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapSourceError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MapSourceError::Address)?;
        let path: Vec<_> = parts.path_segments().collect();
        if parts.scheme() != "map"
            || parts.authority() != "source"
            || parts.has_query()
            || path.len() != 1
        {
            return Err(MapSourceError::Address);
        }
        let address =
            Self::new(MapSourceId::parse(path[0].as_ref()).map_err(|_| MapSourceError::Address)?);
        if address.as_str() != value {
            return Err(MapSourceError::Address);
        }
        Ok(address)
    }
    pub fn id(&self) -> &MapSourceId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapSourcesUri {
    wire: ResourceUri,
    cursor: Option<MapSourceCursor>,
}
impl MapSourcesUri {
    pub const ROOT: &str = "map://sources";
    pub const TEMPLATE: &str = "map://sources{?cursor}";

    pub fn new(cursor: Option<MapSourceCursor>) -> Self {
        let mut builder = ResourceUriBuilder::new(Self::ROOT).expect("declared Map root");
        if let Some(cursor) = &cursor {
            builder = builder
                .query_pair("cursor", cursor.as_str())
                .expect("typed cursor");
        }
        Self {
            wire: builder.build().expect("typed source collection"),
            cursor,
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MapSourceError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MapSourceError::Address)?;
        if parts.scheme() != "map"
            || parts.authority() != "sources"
            || parts.path_segments().next().is_some()
        {
            return Err(MapSourceError::Address);
        }
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(MapSourceError::Address);
            }
            Some(MapSourceCursor::parse(
                query.get("cursor").ok_or(MapSourceError::Address)?.clone(),
            )?)
        } else {
            None
        };
        let address = Self::new(cursor);
        if address.as_str() != value {
            return Err(MapSourceError::Address);
        }
        Ok(address)
    }
    pub fn cursor(&self) -> Option<&MapSourceCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

macro_rules! address_wire {
    ($ty:ty) => {
        impl TryFrom<String> for $ty {
            type Error = MapSourceError;
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
            type Error = MapSourceError;
            fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(value.as_str())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok(self.wire.clone())
            }
        }
    };
}
address_wire!(MapSourceUri);
address_wire!(MapSourcesUri);
