//! Stream route vocabulary composed with the shared URI parser and builder.
use std::fmt;

use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

use super::{ModelId, PipelineId, RunCursor, RunId, SessionCursor, SessionId, StreamContractError};
use crate::uris;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamDocument {
    Agents,
    Design,
}
impl StreamDocument {
    pub fn parse(value: &str) -> Result<Self, StreamContractError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(StreamContractError::InvalidResource),
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
#[resource(template = "stream://pipeline/{pipeline_id}", error = StreamContractError, route_error = |_| StreamContractError::InvalidResource, wire, schema = resource_string_schema, schema_inline)]
pub struct PipelineUri(#[resource(variable = "pipeline_id", error = |error| error)] PipelineId);
impl PipelineUri {
    pub fn new(id: PipelineId) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &PipelineId {
        &self.0
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let uri =
            ResourceUri::new(value.as_ref()).map_err(|_| StreamContractError::InvalidResource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(&self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Stream resource")
    }
}
impl fmt::Display for PipelineUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(template = "stream://model/{model_id}", error = StreamContractError, route_error = |_| StreamContractError::InvalidResource, wire, schema = resource_string_schema, schema_inline)]
pub struct ModelUri(#[resource(variable = "model_id", error = |error| error)] ModelId);
impl ModelUri {
    pub fn new(id: ModelId) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &ModelId {
        &self.0
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let uri =
            ResourceUri::new(value.as_ref()).map_err(|_| StreamContractError::InvalidResource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(&self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Stream resource")
    }
}
impl fmt::Display for ModelUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(template = "stream://run/{run_id}", error = StreamContractError, route_error = |_| StreamContractError::InvalidResource, wire, schema = resource_string_schema, schema_inline)]
pub struct RunUri(#[resource(variable = "run_id", error = |error| error)] RunId);
impl RunUri {
    pub fn new(id: RunId) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &RunId {
        &self.0
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let uri =
            ResourceUri::new(value.as_ref()).map_err(|_| StreamContractError::InvalidResource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(&self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Stream resource")
    }
}
impl fmt::Display for RunUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(template = "stream://run/{run_id}/results", error = StreamContractError, route_error = |_| StreamContractError::InvalidResource, wire, schema = resource_string_schema, schema_inline)]
pub struct RunResultsUri(#[resource(variable = "run_id", error = |error| error)] RunId);
impl RunResultsUri {
    pub fn new(id: RunId) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &RunId {
        &self.0
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let uri =
            ResourceUri::new(value.as_ref()).map_err(|_| StreamContractError::InvalidResource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(&self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Stream resource")
    }
}
impl fmt::Display for RunResultsUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(template = "stream://session/{session_id}", error = StreamContractError, route_error = |_| StreamContractError::InvalidResource, wire, schema = resource_string_schema, schema_inline)]
pub struct SessionUri(#[resource(variable = "session_id", error = |error| error)] SessionId);
impl SessionUri {
    pub fn new(id: SessionId) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &SessionId {
        &self.0
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let uri =
            ResourceUri::new(value.as_ref()).map_err(|_| StreamContractError::InvalidResource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(&self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Stream resource")
    }
}
impl fmt::Display for SessionUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(template = "stream://session/{session_id}/results", error = StreamContractError, route_error = |_| StreamContractError::InvalidResource, wire, schema = resource_string_schema, schema_inline)]
pub struct SessionResultsUri(#[resource(variable = "session_id", error = |error| error)] SessionId);
impl SessionResultsUri {
    pub fn new(id: SessionId) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &SessionId {
        &self.0
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let uri =
            ResourceUri::new(value.as_ref()).map_err(|_| StreamContractError::InvalidResource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(&self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Stream resource")
    }
}
impl fmt::Display for SessionResultsUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, veoveo_types::ResourceAddress)]
#[serde(try_from = "String", into = "String")]
#[resource(template = "stream://session/{session_id}/preview", error = StreamContractError, route_error = |_| StreamContractError::InvalidResource, wire, schema = resource_string_schema, schema_inline)]
pub struct SessionPreviewUri(#[resource(variable = "session_id", error = |error| error)] SessionId);
impl SessionPreviewUri {
    pub fn new(id: SessionId) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &SessionId {
        &self.0
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let uri =
            ResourceUri::new(value.as_ref()).map_err(|_| StreamContractError::InvalidResource)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(&self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Stream resource")
    }
}
impl fmt::Display for SessionPreviewUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}

/// Every resource constructor keeps its owning ID and collection cursor type.
/// ```compile_fail
/// use veoveo_stream_mcp::contract::{PipelineUri, ModelId};
/// PipelineUri::new(ModelId::parse("detector").unwrap());
/// ```
/// ```compile_fail
/// use veoveo_stream_mcp::contract::{RunUri, SessionId};
/// RunUri::new(SessionId::parse("01983da0-0000-7000-8000-000000000000").unwrap());
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum StreamResource {
    Docs,
    Document(StreamDocument),
    Contract,
    LiveApp,
    Pipelines,
    Pipeline(PipelineUri),
    Models,
    Model(ModelUri),
    Runs(Option<RunCursor>),
    Run(RunUri),
    RunResults(RunResultsUri),
    Sessions(Option<SessionCursor>),
    Session(SessionUri),
    SessionResults(SessionResultsUri),
    SessionPreview(SessionPreviewUri),
    Artifact(ArtifactId),
}
impl StreamResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let value = value.as_ref();
        let invalid = || StreamContractError::InvalidResource;
        let parts = ResourceUriParts::parse(value).map_err(|_| invalid())?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let path = path.iter().map(|p| p.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), path.as_slice()) {
            ("stream", "runs", []) => Self::Runs(cursor(&parts, RunCursor::parse)?),
            ("stream", "sessions", []) => Self::Sessions(cursor(&parts, SessionCursor::parse)?),
            _ if parts.has_query() => return Err(invalid()),
            ("stream", "docs", []) => Self::Docs,
            ("stream", "docs", [id]) => Self::Document(StreamDocument::parse(id)?),
            ("stream", "contract", []) => Self::Contract,
            ("ui", "stream", ["live.html"]) => Self::LiveApp,
            ("stream", "pipelines", []) => Self::Pipelines,
            ("stream", "pipeline", [id]) => {
                Self::Pipeline(PipelineUri::new(PipelineId::parse(*id)?))
            }
            ("stream", "models", []) => Self::Models,
            ("stream", "model", [id]) => Self::Model(ModelUri::new(ModelId::parse(*id)?)),
            ("stream", "run", [id]) => Self::Run(RunUri::new(RunId::parse(id)?)),
            ("stream", "run", [id, "results"]) => {
                Self::RunResults(RunResultsUri::new(RunId::parse(id)?))
            }
            ("stream", "session", [id]) => Self::Session(SessionUri::new(SessionId::parse(id)?)),
            ("stream", "session", [id, "results"]) => {
                Self::SessionResults(SessionResultsUri::new(SessionId::parse(id)?))
            }
            ("stream", "session", [id, "preview"]) => {
                Self::SessionPreview(SessionPreviewUri::new(SessionId::parse(id)?))
            }
            ("stream", "artifact", [_]) => Self::Artifact(
                ArtifactUri::parse(value)
                    .map_err(|_| invalid())?
                    .artifact_id(),
            ),
            _ => return Err(invalid()),
        };
        if resource.to_uri()?.as_str() != value {
            return Err(invalid());
        }
        Ok(resource)
    }
    pub fn subscription_run(&self) -> Option<RunId> {
        match self {
            Self::Run(uri) => Some(*uri.id()),
            Self::RunResults(uri) => Some(*uri.id()),
            _ => None,
        }
    }
    pub fn subscription_session(&self) -> Option<SessionId> {
        match self {
            Self::Session(uri) => Some(*uri.id()),
            Self::SessionResults(uri) => Some(*uri.id()),
            Self::SessionPreview(uri) => Some(*uri.id()),
            _ => None,
        }
    }
}
fn cursor<T>(
    parts: &ResourceUriParts,
    parse: impl FnOnce(String) -> Result<T, StreamContractError>,
) -> Result<Option<T>, StreamContractError> {
    match parts
        .query_parameters()
        .iter()
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] if !parts.has_query() => Ok(None),
        [(name, value)] if name.as_str() == "cursor" => parse(value.to_string()).map(Some),
        _ => Err(StreamContractError::InvalidResource),
    }
}
fn collection(root: &str, cursor: Option<&str>) -> Result<ResourceUri, StreamContractError> {
    let mut builder =
        ResourceUriBuilder::new(root).map_err(|_| StreamContractError::InvalidResource)?;
    if let Some(cursor) = cursor {
        builder = builder
            .query_pair("cursor", cursor)
            .map_err(|_| StreamContractError::InvalidResource)?;
    }
    builder
        .build()
        .map_err(|_| StreamContractError::InvalidResource)
}
impl ResourceAddress for StreamResource {
    type Error = StreamContractError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let literal =
            |value| ResourceUri::new(value).map_err(|_| StreamContractError::InvalidResource);
        match self {
            Self::Docs => literal(uris::DOCS_URI),
            Self::Document(id) => ResourceUriBuilder::new(uris::DOCS_URI)
                .map_err(|_| StreamContractError::InvalidResource)?
                .segment(
                    UriSegment::new(id.as_str())
                        .map_err(|_| StreamContractError::InvalidResource)?,
                )
                .build()
                .map_err(|_| StreamContractError::InvalidResource),
            Self::Contract => literal(uris::CONTRACT_URI),
            Self::LiveApp => literal(uris::LIVE_APP_URI),
            Self::Pipelines => literal(uris::PIPELINES_URI),
            Self::Models => literal(uris::MODELS_URI),
            Self::Pipeline(uri) => Ok(uri.to_uri()),
            Self::Model(uri) => Ok(uri.to_uri()),
            Self::Run(uri) => Ok(uri.to_uri()),
            Self::RunResults(uri) => Ok(uri.to_uri()),
            Self::Session(uri) => Ok(uri.to_uri()),
            Self::SessionResults(uri) => Ok(uri.to_uri()),
            Self::SessionPreview(uri) => Ok(uri.to_uri()),
            Self::Runs(cursor) => {
                collection(uris::RUNS_URI, cursor.as_ref().map(RunCursor::as_str))
            }
            Self::Sessions(cursor) => collection(
                uris::SESSIONS_URI,
                cursor.as_ref().map(SessionCursor::as_str),
            ),
            Self::Artifact(id) => ArtifactUri::presented(&uris::SCHEME, *id)
                .to_uri()
                .map_err(|_| StreamContractError::InvalidResource),
        }
    }
}
impl TryFrom<String> for StreamResource {
    type Error = StreamContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<StreamResource> for String {
    fn from(value: StreamResource) -> Self {
        value
            .to_uri()
            .expect("admitted Stream resource")
            .to_string()
    }
}
impl schemars::JsonSchema for StreamResource {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "StreamResource".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        resource_string_schema(generator)
    }
}

fn resource_string_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <String as schemars::JsonSchema>::json_schema(generator)
}
