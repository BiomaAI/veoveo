//! Recording-owned routes built and parsed through foundational URI components.
use super::{RecordingCatalogCursor, RecordingContractError, RecordingId, ids::string_schema};
use crate::uris;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingDocument {
    Agents,
    Design,
}
impl RecordingDocument {
    pub fn parse(value: &str) -> Result<Self, RecordingContractError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(RecordingContractError::Resource),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
}

macro_rules! address {
    ($name:ident, $variant:ident, $tail:expr) => {
        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name {
            id: RecordingId,
            wire: ResourceUri,
        }
        impl $name {
            pub fn new(id: RecordingId) -> Self {
                let mut b = ResourceUriBuilder::new("recording://recordings")
                    .expect("declared root")
                    .segment(UriSegment::new(id.to_string()).expect("typed recording identity"));
                if let Some(tail) = $tail {
                    b = b.segment(UriSegment::new(tail).expect("declared child"));
                }
                Self {
                    id,
                    wire: b.build().expect("typed recording address"),
                }
            }
            pub fn id(&self) -> RecordingId {
                self.id
            }
            pub fn as_str(&self) -> &str {
                self.wire.as_str()
            }
            pub fn as_resource_uri(&self) -> &ResourceUri {
                &self.wire
            }
            pub fn parse(value: impl AsRef<str>) -> Result<Self, RecordingContractError> {
                match RecordingResource::parse(value)? {
                    RecordingResource::$variant(uri) => Ok(uri),
                    _ => Err(RecordingContractError::Resource),
                }
            }
        }
        impl std::str::FromStr for $name {
            type Err = RecordingContractError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }
        impl TryFrom<String> for $name {
            type Error = RecordingContractError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.wire.into()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.wire.fmt(f)
            }
        }
        impl ResourceAddress for $name {
            type Error = RecordingContractError;
            fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(value.as_str())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok(self.wire.clone())
            }
        }
        string_schema!($name);
    };
}
address!(RecordingUri, Recording, None::<&str>);
address!(RecordingLayersUri, Layers, Some("layers"));

/// All hosted Recording resource families.
/// ```compile_fail
/// use veoveo_recording_mcp::contract::RecordingUri;
/// RecordingUri::new("01983da0-0000-7000-8000-000000000000");
/// ```
/// ```compile_fail
/// use veoveo_recording_mcp::contract::RecordingUri;
/// RecordingUri::new(veoveo_types::TaskId::new());
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum RecordingResource {
    Docs,
    Document(RecordingDocument),
    Contract,
    Explorer,
    Catalog(Option<RecordingCatalogCursor>),
    Recording(RecordingUri),
    Layers(RecordingLayersUri),
}
impl RecordingResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, RecordingContractError> {
        let value = value.as_ref();
        let invalid = || RecordingContractError::Resource;
        let parts = ResourceUriParts::parse(value).map_err(|_| invalid())?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let path = path.iter().map(|p| p.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), path.as_slice()) {
            ("recording", "catalog", []) => {
                let cursor = match parts
                    .query_parameters()
                    .iter()
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    [] if !parts.has_query() => None,
                    [(key, value)] if key.as_str() == "cursor" => {
                        Some(RecordingCatalogCursor::parse((*value).clone())?)
                    }
                    _ => return Err(invalid()),
                };
                Self::Catalog(cursor)
            }
            _ if parts.has_query() => return Err(invalid()),
            ("recording", "docs", []) => Self::Docs,
            ("recording", "docs", [id]) => Self::Document(RecordingDocument::parse(id)?),
            ("recording", "contract", []) => Self::Contract,
            ("ui", "recording", ["explorer.html"]) => Self::Explorer,
            ("recording", "recordings", [id]) => {
                Self::Recording(RecordingUri::new(RecordingId::parse(id)?))
            }
            ("recording", "recordings", [id, "layers"]) => {
                Self::Layers(RecordingLayersUri::new(RecordingId::parse(id)?))
            }
            _ => return Err(invalid()),
        };
        if resource.to_uri()?.as_str() != value {
            return Err(invalid());
        }
        Ok(resource)
    }
}
impl ResourceAddress for RecordingResource {
    type Error = RecordingContractError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let root = match self {
            Self::Docs => uris::DOCS_URI,
            Self::Contract => uris::CONTRACT_URI,
            Self::Explorer => uris::EXPLORER_APP_URI,
            Self::Document(doc) => {
                return ResourceUriBuilder::new(uris::DOCS_URI)
                    .expect("declared docs root")
                    .segment(UriSegment::new(doc.as_str()).expect("declared document"))
                    .build()
                    .map_err(|_| RecordingContractError::Resource);
            }
            Self::Catalog(cursor) => {
                let mut b =
                    ResourceUriBuilder::new(uris::CATALOG_URI).expect("declared catalog root");
                if let Some(cursor) = cursor {
                    b = b
                        .query_pair("cursor", cursor.as_str())
                        .map_err(|_| RecordingContractError::Resource)?;
                }
                return b.build().map_err(|_| RecordingContractError::Resource);
            }
            Self::Recording(uri) => return uri.to_uri(),
            Self::Layers(uri) => return uri.to_uri(),
        };
        ResourceUri::new(root).map_err(|_| RecordingContractError::Resource)
    }
}
impl TryFrom<String> for RecordingResource {
    type Error = RecordingContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<RecordingResource> for String {
    fn from(value: RecordingResource) -> Self {
        value.to_uri().expect("typed Recording resource").into()
    }
}
impl fmt::Display for RecordingResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().map_err(|_| fmt::Error)?.fmt(f)
    }
}
string_schema!(RecordingResource);
