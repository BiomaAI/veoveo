//! Speech addresses retain the specific domain identity through construction.
use super::{DictationSessionId, TranscriptionId};

use veoveo_artifact_contract::ArtifactId;
use veoveo_types::{ResourceFieldCodec, ResourceRouteError, ResourceUriError, TaskResourceAddress};

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

#[veoveo_types::resource_address(components(ResourceUriErrorAddresses), template = "speech://transcript/{task_id}", traits = copied, parse = borrowed, to_uri = copied)]
pub struct TranscriptionUri(
    #[resource(variable = "task_id", error = |_| ResourceUriError::DisallowedComponent, accessor = id, owned_accessor)]
     TranscriptionId,
);
#[veoveo_types::resource_address(components(ResourceUriErrorAddresses), template = "speech://dictation/{id}", traits = copied, parse = borrowed, to_uri = copied)]
pub struct DictationUri(
    #[resource(variable = "id", error = |_| ResourceUriError::DisallowedComponent, accessor = id, owned_accessor)]
     DictationSessionId,
);
/// ```compile_fail
/// use veoveo_speech_contract::{DictationSessionId, TranscriptionUri};
/// TranscriptionUri::new(DictationSessionId::new());
/// ```
/// ```compile_fail
/// use veoveo_speech_contract::DictationUri;
/// DictationUri::new("01983da0-0000-7000-8000-000000000001");
/// ```
#[veoveo_types::resource_address(routes(ResourceUriErrorAddresses), traits = copied, parse = borrowed, to_uri = copied)]
pub enum SpeechResource {
    #[resource(template = "speech://capabilities")]
    Capabilities,
    #[resource(template = "speech://docs")]
    Docs,
    #[resource(template = "speech://docs/{doc_id}")]
    Document(#[resource(variable = "doc_id", codec = SpeechDocumentCodec)] SpeechDocument),
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

#[doc(hidden)]
pub struct ResourceUriErrorAddresses;
impl veoveo_types::ResourceProfile for ResourceUriErrorAddresses {
    type Error = ResourceUriError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, error| speech_route_error(error),
        };
    const SCHEMA: Option<veoveo_types::ResourceSchema> = Some(veoveo_types::ResourceSchema {
        schema: |name, _generator| match name {
            "TranscriptionUri" => {
                schemars::json_schema!({ "type": "string", "pattern": "^speech://transcript/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}(?![\\s\\S])" })
            }
            "DictationUri" => {
                schemars::json_schema!({ "type": "string", "pattern": "^speech://dictation/[0-9a-f]{8}-[0-9a-f]{4}-[47][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}(?![\\s\\S])" })
            }
            _ => unreachable!("owner schema declaration"),
        },
        inline: false,
    });
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
