//! Recording-owned routes built and parsed through foundational URI components.
use super::{RecordingCatalogCursor, RecordingContractError, RecordingId, ids::string_schema};
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_types::{Identity, ResourceAddress, ResourceFieldCodec, ResourceUri};

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(template="recording://recordings/{recording_id}", error=RecordingContractError, route_error=|_| RecordingContractError::Resource, wire, schema=string_schema, schema_inline)]
pub struct RecordingUri {
    #[resource(variable="recording_id", error=|error| error)]
    id: RecordingId,
    #[resource(cache)]
    wire: ResourceUri,
}
impl RecordingUri {
    pub fn new(id: RecordingId) -> Self {
        Self::resource_from_parts(id).expect("typed Recording address")
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
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| RecordingContractError::Resource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
}
impl std::str::FromStr for RecordingUri {
    type Err = RecordingContractError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl fmt::Display for RecordingUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.wire.fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(template="recording://recordings/{recording_id}/layers", error=RecordingContractError, route_error=|_| RecordingContractError::Resource, wire, schema=string_schema, schema_inline)]
pub struct RecordingLayersUri {
    #[resource(variable="recording_id", error=|error| error)]
    id: RecordingId,
    #[resource(cache)]
    wire: ResourceUri,
}
impl RecordingLayersUri {
    pub fn new(id: RecordingId) -> Self {
        Self::resource_from_parts(id).expect("typed Recording address")
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
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| RecordingContractError::Resource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
}
impl std::str::FromStr for RecordingLayersUri {
    type Err = RecordingContractError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl fmt::Display for RecordingLayersUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.wire.fmt(f)
    }
}

/// All hosted Recording resource families.
/// ```compile_fail
/// use veoveo_recording_contract::RecordingUri;
/// RecordingUri::new("01983da0-0000-7000-8000-000000000000");
/// ```
/// ```compile_fail
/// use veoveo_recording_contract::RecordingUri;
/// RecordingUri::new(veoveo_types::TaskId::new());
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(error=RecordingContractError, route_error=|_| RecordingContractError::Resource, wire, schema=string_schema, schema_inline)]
pub enum RecordingResource {
    #[resource(template = "recording://docs")]
    Docs,
    #[resource(template = "recording://docs/{doc_id}")]
    Document(
        #[resource(variable="doc_id", codec=DocumentCodec, error=|error| error)] RecordingDocument,
    ),
    #[resource(template = "recording://contract")]
    Contract,
    #[resource(template = "ui://recording/explorer.html")]
    Explorer,
    #[resource(template = "recording://catalog{?cursor}")]
    Catalog(
        #[resource(variable="cursor", codec=CatalogCursorCodec, error=|error| error)]
        Option<RecordingCatalogCursor>,
    ),
    #[resource(template = "recording://recordings/{recording_id}")]
    Recording(
        #[resource(variable="recording_id", codec=RecordingUriCodec, error=|error| error)]
        RecordingUri,
    ),
    #[resource(template = "recording://recordings/{recording_id}/layers")]
    Layers(
        #[resource(variable="recording_id", codec=RecordingLayersCodec, error=|error| error)]
        RecordingLayersUri,
    ),
}
impl RecordingResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, RecordingContractError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| RecordingContractError::Resource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
}
struct DocumentCodec;
impl ResourceFieldCodec<RecordingDocument> for DocumentCodec {
    type Error = RecordingContractError;
    fn parse(value: &str) -> Result<RecordingDocument, Self::Error> {
        RecordingDocument::parse(value)
    }
    fn text(value: &RecordingDocument) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
struct CatalogCursorCodec;
impl ResourceFieldCodec<RecordingCatalogCursor> for CatalogCursorCodec {
    type Error = RecordingContractError;
    fn parse(value: &str) -> Result<RecordingCatalogCursor, Self::Error> {
        RecordingCatalogCursor::parse(value)
    }
    fn text(value: &RecordingCatalogCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
struct RecordingUriCodec;
impl ResourceFieldCodec<RecordingUri> for RecordingUriCodec {
    type Error = RecordingContractError;
    fn parse(value: &str) -> Result<RecordingUri, Self::Error> {
        RecordingId::parse(value).map(RecordingUri::new)
    }
    fn text(value: &RecordingUri) -> std::borrow::Cow<'_, str> {
        value.id.identity_text()
    }
}
struct RecordingLayersCodec;
impl ResourceFieldCodec<RecordingLayersUri> for RecordingLayersCodec {
    type Error = RecordingContractError;
    fn parse(value: &str) -> Result<RecordingLayersUri, Self::Error> {
        RecordingId::parse(value).map(RecordingLayersUri::new)
    }
    fn text(value: &RecordingLayersUri) -> std::borrow::Cow<'_, str> {
        value.id.identity_text()
    }
}
impl fmt::Display for RecordingResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().map_err(|_| fmt::Error)?.fmt(f)
    }
}
