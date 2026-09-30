//! Speech addresses retain the specific domain identity through construction.
use super::{DictationSessionId, TranscriptionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_artifact_contract::ArtifactId;
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriError, ResourceUriParts,
    TaskResourceAddress, UriSegment,
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

macro_rules! address {
    ($name:ident, $id:ty, $variant:ident, $root:literal, $versions:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name($id);
        impl $name {
            pub fn new(id: $id) -> Self {
                Self(id)
            }
            pub fn id(self) -> $id {
                self.0
            }
            pub fn to_uri(self) -> ResourceUri {
                SpeechResource::$variant(self.0).to_uri()
            }
            pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
                match SpeechResource::parse(value)? {
                    SpeechResource::$variant(id) => Ok(Self(id)),
                    _ => Err(ResourceUriError::DisallowedComponent),
                }
            }
        }
        impl TryFrom<String> for $name {
            type Error = ResourceUriError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(&value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.to_string()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                (*self).to_uri().fmt(f)
            }
        }
        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({ "type": "string", "pattern": concat!("^", $root,
                    "/[0-9a-f]{8}-[0-9a-f]{4}-", $versions, "[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}(?![\\s\\S])") })
            }
        }
    };
}
address!(
    TranscriptionUri,
    TranscriptionId,
    Transcript,
    "speech://transcript",
    "7"
);
address!(
    DictationUri,
    DictationSessionId,
    Dictation,
    "speech://dictation",
    "[47]"
);

/// ```compile_fail
/// use veoveo_speech_contract::{DictationSessionId, TranscriptionUri};
/// TranscriptionUri::new(DictationSessionId::new());
/// ```
/// ```compile_fail
/// use veoveo_speech_contract::DictationUri;
/// DictationUri::new("01983da0-0000-7000-8000-000000000001");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechResource {
    Capabilities,
    Docs,
    Document(SpeechDocument),
    Contract,
    Transcript(TranscriptionId),
    Dictation(DictationSessionId),
    Artifact(ArtifactId),
}
impl SpeechResource {
    pub const TRANSCRIPT_TEMPLATE: &'static str = "speech://transcript/{task_id}";
    pub const DICTATION_TEMPLATE: &'static str = "speech://dictation/{id}";
    pub const ARTIFACT_TEMPLATE: &'static str = "speech://artifact/{artifact_id}";
    pub const DOCUMENT_TEMPLATE: &'static str = "speech://docs/{doc_id}";

    pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
        let invalid = || ResourceUriError::DisallowedComponent;
        let parts = ResourceUriParts::parse(value)?;
        if parts.scheme() != "speech" || parts.has_query() {
            return Err(invalid());
        }
        let segments = parts.path_segments().collect::<Vec<_>>();
        let path = segments.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.authority(), path.as_slice()) {
            ("capabilities", []) => Self::Capabilities,
            ("docs", []) => Self::Docs,
            ("docs", [doc]) => Self::Document(SpeechDocument::parse(doc)?),
            ("contract", []) => Self::Contract,
            ("transcript", [id]) => {
                Self::Transcript(TranscriptionId::parse(id).map_err(|_| invalid())?)
            }
            ("dictation", [id]) => {
                Self::Dictation(DictationSessionId::parse(id).map_err(|_| invalid())?)
            }
            ("artifact", [id]) => Self::Artifact(ArtifactId::parse(id).map_err(|_| invalid())?),
            _ => return Err(invalid()),
        };
        if resource.to_uri().as_str() != value {
            return Err(invalid());
        }
        Ok(resource)
    }
    pub fn to_uri(self) -> ResourceUri {
        let (root, segment) = match self {
            Self::Capabilities => ("speech://capabilities", None),
            Self::Docs => ("speech://docs", None),
            Self::Document(doc) => ("speech://docs", Some(doc.as_str().to_owned())),
            Self::Contract => ("speech://contract", None),
            Self::Transcript(id) => ("speech://transcript", Some(id.to_string())),
            Self::Dictation(id) => ("speech://dictation", Some(id.to_string())),
            Self::Artifact(id) => ("speech://artifact", Some(id.to_string())),
        };
        let mut builder = ResourceUriBuilder::new(root).expect("declared Speech root");
        if let Some(segment) = segment {
            builder = builder.segment(UriSegment::new(segment).expect("admitted Speech component"));
        }
        builder.build().expect("declared Speech resource")
    }
}
impl ResourceAddress for SpeechResource {
    type Error = ResourceUriError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok((*self).to_uri())
    }
}
impl ResourceAddress for TranscriptionUri {
    type Error = ResourceUriError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok((*self).to_uri())
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
