//! Reason route vocabulary composed with the shared URI parser and builder.
use std::fmt;

use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

use super::{
    AnalysisCursor, AnalysisId, ModelId, PipelineId, ReasonContractError, ids::string_schema,
};
use crate::uris;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasonDocument {
    Agents,
    Design,
}
impl ReasonDocument {
    pub fn parse(value: &str) -> Result<Self, ReasonContractError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(ReasonContractError::InvalidResource),
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
    ($name:ident, $id:ty, $variant:ident, $root:literal, $tail:expr) => {
        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name($id);
        impl $name {
            pub fn new(id: $id) -> Self {
                Self(id)
            }
            pub fn id(&self) -> &$id {
                &self.0
            }
            pub fn parse(value: impl AsRef<str>) -> Result<Self, ReasonContractError> {
                match ReasonResource::parse(value)? {
                    ReasonResource::$variant(uri) => Ok(uri),
                    _ => Err(ReasonContractError::InvalidResource),
                }
            }
            pub fn to_uri(&self) -> ResourceUri {
                let mut builder = ResourceUriBuilder::new($root)
                    .expect("declared Reason root")
                    .segment(UriSegment::new(self.0.to_string()).expect("admitted Reason ID"));
                if let Some(tail) = $tail {
                    builder =
                        builder.segment(UriSegment::new(tail).expect("declared Reason child"));
                }
                builder.build().expect("admitted Reason resource")
            }
        }
        impl TryFrom<String> for $name {
            type Error = ReasonContractError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.to_uri().to_string()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.to_uri().fmt(f)
            }
        }
        string_schema!($name);
    };
}
address!(
    PipelineUri,
    PipelineId,
    Pipeline,
    "reason://pipeline",
    None::<&str>
);
address!(ModelUri, ModelId, Model, "reason://model", None::<&str>);
address!(
    AnalysisUri,
    AnalysisId,
    Analysis,
    "reason://analysis",
    None::<&str>
);
address!(
    ResultsUri,
    AnalysisId,
    Results,
    "reason://analysis",
    Some("results")
);

/// Each variant admits only its owning identifier and route parameters.
///
/// ```compile_fail
/// use veoveo_reason_mcp::contract::{ModelId, PipelineUri};
/// PipelineUri::new(ModelId::parse("world-model").unwrap());
/// ```
/// ```compile_fail
/// use veoveo_reason_mcp::contract::AnalysisUri;
/// AnalysisUri::new("01983da0-0000-7000-8000-000000000000");
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ReasonResource {
    Docs,
    Document(ReasonDocument),
    Contract,
    AnalysesApp,
    Pipelines,
    Pipeline(PipelineUri),
    Models,
    Model(ModelUri),
    Analyses(Option<AnalysisCursor>),
    Analysis(AnalysisUri),
    Results(ResultsUri),
    Artifact(ArtifactId),
    Knowledge(super::FindingResource),
}
impl ReasonResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ReasonContractError> {
        let value = value.as_ref();
        let invalid = || ReasonContractError::InvalidResource;
        let parts = ResourceUriParts::parse(value).map_err(|_| invalid())?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let path = path.iter().map(|p| p.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), path.as_slice()) {
            ("reason", "knowledge", _) => Self::Knowledge(super::FindingResource::parse(value)?),
            ("reason", "analyses", []) => {
                let cursor = match parts
                    .query_parameters()
                    .iter()
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    [] if !parts.has_query() => None,
                    [(name, cursor)] if name.as_str() == "cursor" => {
                        Some(AnalysisCursor::parse(cursor.as_str())?)
                    }
                    _ => return Err(invalid()),
                };
                Self::Analyses(cursor)
            }
            _ if parts.has_query() => return Err(invalid()),
            ("reason", "docs", []) => Self::Docs,
            ("reason", "docs", [id]) => Self::Document(ReasonDocument::parse(id)?),
            ("reason", "contract", []) => Self::Contract,
            ("ui", "reason", ["analyses.html"]) => Self::AnalysesApp,
            ("reason", "pipelines", []) => Self::Pipelines,
            ("reason", "pipeline", [id]) => {
                Self::Pipeline(PipelineUri::new(PipelineId::parse(*id)?))
            }
            ("reason", "models", []) => Self::Models,
            ("reason", "model", [id]) => Self::Model(ModelUri::new(ModelId::parse(*id)?)),
            ("reason", "analysis", [id]) => {
                Self::Analysis(AnalysisUri::new(AnalysisId::parse(id)?))
            }
            ("reason", "analysis", [id, "results"]) => {
                Self::Results(ResultsUri::new(AnalysisId::parse(id)?))
            }
            ("reason", "artifact", [_]) => Self::Artifact(
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
    pub fn subscription_analysis(&self) -> Option<AnalysisId> {
        match self {
            Self::Analysis(uri) => Some(*uri.id()),
            Self::Results(uri) => Some(*uri.id()),
            _ => None,
        }
    }
}
impl ResourceAddress for ReasonResource {
    type Error = ReasonContractError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let literal =
            |value| ResourceUri::new(value).map_err(|_| ReasonContractError::InvalidResource);
        match self {
            Self::Docs => literal(uris::DOCS_URI),
            Self::Document(id) => ResourceUriBuilder::new(uris::DOCS_URI)
                .map_err(|_| ReasonContractError::InvalidResource)?
                .segment(
                    UriSegment::new(id.as_str())
                        .map_err(|_| ReasonContractError::InvalidResource)?,
                )
                .build()
                .map_err(|_| ReasonContractError::InvalidResource),
            Self::Contract => literal(uris::CONTRACT_URI),
            Self::Knowledge(address) => address.to_uri(),
            Self::AnalysesApp => literal(uris::ANALYSES_APP_URI),
            Self::Pipelines => literal(uris::PIPELINES_URI),
            Self::Models => literal(uris::MODELS_URI),
            Self::Pipeline(uri) => Ok(uri.to_uri()),
            Self::Model(uri) => Ok(uri.to_uri()),
            Self::Analysis(uri) => Ok(uri.to_uri()),
            Self::Results(uri) => Ok(uri.to_uri()),
            Self::Analyses(cursor) => {
                let mut builder = ResourceUriBuilder::new(uris::ANALYSES_URI)
                    .map_err(|_| ReasonContractError::InvalidResource)?;
                if let Some(cursor) = cursor {
                    builder = builder
                        .query_pair("cursor", cursor.as_str())
                        .map_err(|_| ReasonContractError::InvalidResource)?;
                }
                builder
                    .build()
                    .map_err(|_| ReasonContractError::InvalidResource)
            }
            Self::Artifact(id) => ArtifactUri::presented(&uris::SCHEME, *id)
                .to_uri()
                .map_err(|_| ReasonContractError::InvalidResource),
        }
    }
}
impl TryFrom<String> for ReasonResource {
    type Error = ReasonContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<ReasonResource> for String {
    fn from(value: ReasonResource) -> Self {
        value
            .to_uri()
            .expect("admitted Reason resource")
            .to_string()
    }
}
string_schema!(ReasonResource);
