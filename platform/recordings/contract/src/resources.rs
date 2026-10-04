//! Recording-owned routes built and parsed through foundational URI components.
use super::{RecordingCatalogCursor, RecordingContractError, RecordingId, ids::string_schema};

use veoveo_types::{Identity, ResourceFieldCodec, ResourceUri};

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

#[veoveo_types::resource_address(cached(RecordingContractErrorAddresses), template = "recording://recordings/{recording_id}", schema = owner, from_str, resource_uri)]
pub struct RecordingUri {
    #[resource(variable="recording_id",  accessor = id, copy_accessor)]
    id: RecordingId,
    #[resource(cache)]
    wire: ResourceUri,
}
#[veoveo_types::resource_address(cached(RecordingContractErrorAddresses), template = "recording://recordings/{recording_id}/layers", schema = owner, from_str, resource_uri)]
pub struct RecordingLayersUri {
    #[resource(variable="recording_id",  accessor = id, copy_accessor)]
    id: RecordingId,
    #[resource(cache)]
    wire: ResourceUri,
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
#[veoveo_types::resource_address(routes(RecordingContractErrorAddresses), schema = owner, wire, display)]
pub enum RecordingResource {
    #[resource(template = "recording://docs")]
    Docs,
    #[resource(template = "recording://docs/{doc_id}")]
    Document(#[resource(variable="doc_id", codec=DocumentCodec)] RecordingDocument),
    #[resource(template = "recording://contract")]
    Contract,
    #[resource(template = "ui://recording/explorer.html")]
    Explorer,
    #[resource(template = "recording://catalog{?cursor}")]
    Catalog(
        #[resource(variable="cursor", codec=CatalogCursorCodec)] Option<RecordingCatalogCursor>,
    ),
    #[resource(template = "recording://recordings/{recording_id}")]
    Recording(#[resource(variable="recording_id", codec=RecordingUriCodec)] RecordingUri),
    #[resource(template = "recording://recordings/{recording_id}/layers")]
    Layers(#[resource(variable="recording_id", codec=RecordingLayersCodec)] RecordingLayersUri),
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

#[doc(hidden)]
pub struct RecordingContractErrorAddresses;
impl veoveo_types::ResourceProfile for RecordingContractErrorAddresses {
    type Error = RecordingContractError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| RecordingContractError::Resource,
        };
    const SCHEMA: Option<veoveo_types::ResourceSchema> = Some(veoveo_types::ResourceSchema {
        schema: |_, generator| string_schema(generator),
        inline: true,
    });
}
