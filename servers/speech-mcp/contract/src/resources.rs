//! Speech addresses retain the specific domain identity through construction.
use super::{DictationSessionId, TranscriptionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_artifact_contract::ArtifactId;
use veoveo_types::{
    ResourceAddress, ResourceFieldCodec, ResourceRouteError, ResourceUri, ResourceUriError,
    TaskResourceAddress,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechDocument {
    Agents,
    Design,
}
impl SpeechDocument {
    pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(ResourceUriError::DisallowedComponent),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template = "speech://transcript/{task_id}", error = ResourceUriError, route_error = speech_route_error, wire)]
pub struct TranscriptionUri(
    #[resource(variable = "task_id", error = |_| ResourceUriError::DisallowedComponent)]
    TranscriptionId,
);
impl TranscriptionUri {
    pub fn new(id: TranscriptionId) -> Self {
        Self(id)
    }
    pub fn id(self) -> TranscriptionId {
        self.0
    }
    pub fn to_uri(self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Speech resource")
    }
    pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
        let uri = ResourceUri::new(value)?;
        <Self as ResourceAddress>::parse(&uri)
    }
}
impl fmt::Display for TranscriptionUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self).to_uri().fmt(f)
    }
}
impl JsonSchema for TranscriptionUri {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TranscriptionUri".into()
    }
    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": "^speech://transcript/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}(?![\\s\\S])" })
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template = "speech://dictation/{id}", error = ResourceUriError, route_error = speech_route_error, wire)]
pub struct DictationUri(
    #[resource(variable = "id", error = |_| ResourceUriError::DisallowedComponent)]
    DictationSessionId,
);
impl DictationUri {
    pub fn new(id: DictationSessionId) -> Self {
        Self(id)
    }
    pub fn id(self) -> DictationSessionId {
        self.0
    }
    pub fn to_uri(self) -> ResourceUri {
        self.resource_components_uri()
            .expect("admitted Speech resource")
    }
    pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
        let uri = ResourceUri::new(value)?;
        <Self as ResourceAddress>::parse(&uri)
    }
}
impl fmt::Display for DictationUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self).to_uri().fmt(f)
    }
}
impl JsonSchema for DictationUri {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DictationUri".into()
    }
    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": "^speech://dictation/[0-9a-f]{8}-[0-9a-f]{4}-[47][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}(?![\\s\\S])" })
    }
}

/// ```compile_fail
/// use veoveo_speech_contract::{DictationSessionId, TranscriptionUri};
/// TranscriptionUri::new(DictationSessionId::new());
/// ```
/// ```compile_fail
/// use veoveo_speech_contract::DictationUri;
/// DictationUri::new("01983da0-0000-7000-8000-000000000001");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::ResourceAddress)]
#[resource(error = ResourceUriError, route_error = speech_route_error)]
pub enum SpeechResource {
    #[resource(template = "speech://capabilities")]
    Capabilities,
    #[resource(template = "speech://docs")]
    Docs,
    #[resource(template = "speech://docs/{doc_id}")]
    Document(
        #[resource(variable = "doc_id", codec = SpeechDocumentCodec, error = |error| error)]
        SpeechDocument,
    ),
    #[resource(template = "speech://contract")]
    Contract,
    #[resource(template = "speech://transcript/{task_id}")]
    Transcript(
        #[resource(variable = "task_id", error = |_| ResourceUriError::DisallowedComponent)]
        TranscriptionId,
    ),
    #[resource(template = "speech://dictation/{id}")]
    Dictation(
        #[resource(variable = "id", error = |_| ResourceUriError::DisallowedComponent)]
        DictationSessionId,
    ),
    #[resource(template = "speech://artifact/{artifact_id}")]
    Artifact(
        #[resource(variable = "artifact_id", error = |_| ResourceUriError::DisallowedComponent)]
        ArtifactId,
    ),
}
impl SpeechResource {
    pub const TRANSCRIPT_TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE_TRANSCRIPT;
    pub const DICTATION_TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE_DICTATION;
    pub const ARTIFACT_TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE_ARTIFACT;
    pub const DOCUMENT_TEMPLATE: &'static str = Self::RESOURCE_TEMPLATE_DOCUMENT;

    pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
        let uri = ResourceUri::new(value)?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn to_uri(self) -> ResourceUri {
        self.resource_components_uri()
            .expect("declared Speech resource")
    }
}

fn speech_route_error(error: ResourceRouteError) -> ResourceUriError {
    match error {
        ResourceRouteError::Uri(error) => error,
        _ => ResourceUriError::DisallowedComponent,
    }
}
struct SpeechDocumentCodec;
impl ResourceFieldCodec<SpeechDocument> for SpeechDocumentCodec {
    type Error = ResourceUriError;
    fn parse(value: &str) -> Result<SpeechDocument, Self::Error> {
        SpeechDocument::parse(value)
    }
    fn text(value: &SpeechDocument) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}

impl TaskResourceAddress for TranscriptionUri {
    fn task_id(&self) -> veoveo_types::TaskId {
        self.0.task_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn families_round_trip_and_reject_noncanonical_or_unrelated_components() {
        for resource in [
            SpeechResource::Capabilities,
            SpeechResource::Docs,
            SpeechResource::Document(SpeechDocument::Agents),
            SpeechResource::Document(SpeechDocument::Design),
            SpeechResource::Contract,
            SpeechResource::Transcript(TranscriptionId::new()),
            SpeechResource::Dictation(DictationSessionId::new()),
            SpeechResource::Artifact(ArtifactId::new()),
        ] {
            let uri = resource.to_uri();
            assert_eq!(SpeechResource::parse(uri.as_str()).unwrap(), resource);
            for tail in ["/", "/extra", "?", "?a=1&a=2", "#fragment"] {
                assert!(SpeechResource::parse(&format!("{uri}{tail}")).is_err());
            }
        }
        for uri in [
            "speech://docs/%61gents",
            "speech://docs/../capabilities",
            "speech://transcript/not-an-id",
            "speech://transcript/01983da0-0000-4000-8000-000000000001",
            "speech://user@docs",
            "speech://docs:80",
            "artifact://docs",
            "speech://docs/unknown",
        ] {
            assert!(SpeechResource::parse(uri).is_err(), "{uri}");
        }
        let transcript = TranscriptionUri::new(TranscriptionId::new());
        assert!(DictationUri::parse(&transcript.to_string()).is_err());
        assert_eq!(
            serde_json::from_value::<TranscriptionUri>(serde_json::to_value(transcript).unwrap())
                .unwrap(),
            transcript
        );
    }
}
