mod identity;
mod resources;
mod scopes;
mod task_kind;
pub use identity::{DictationSessionId, SpeechIdentityError, TranscriptionId};
pub use resources::{DictationUri, SpeechDocument, SpeechResource, TranscriptionUri};
pub use scopes::SpeechScope;
pub use task_kind::SpeechTaskKind;
pub mod dictation;
pub mod transcript;
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use transcript::Transcript;
use veoveo_artifact_contract::{ArtifactId, ArtifactMetadata, ArtifactUri};

pub static ARTIFACT_SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::parse("speech").expect("declared Speech scheme")
    });

pub const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const TRANSCRIPT_MIME: &str = "application/json";
pub const TRANSCRIPT_SCHEMA: &str = "veoveo.ai/speech-transcript/v2";

#[derive(JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct SchemaBundle {
    start_dictation: dictation::StartDictation,
    dictation_id: dictation::DictationId,
    dictation: dictation::DictationSnapshot,
    transcribe: TranscribeRequest,
    output: TranscriptionOutput,
    document: TranscriptDocument,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(SchemaBundle)
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscribeRequest {
    /// `artifact://` URI of an uploaded audio or video file you can read.
    pub artifact_uri: ArtifactUri,
}

impl TranscribeRequest {
    pub fn source(&self) -> Result<ArtifactId> {
        ensure!(
            self.artifact_uri.as_str().len() <= 256,
            "source URI exceeds limit"
        );
        Ok(self.artifact_uri.artifact_id())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "TranscriptionOutput")]
pub struct TranscriptionOutputValue {
    pub result_uri: TranscriptionUri,
    pub source_artifact_uri: ArtifactUri,
    pub transcript: ArtifactMetadata,
    pub captions: ArtifactMetadata,
    pub duration_seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "TranscriptDocument")]
pub struct TranscriptDocumentValue {
    #[schemars(schema_with = "transcript_format_schema")]
    pub schema: String,
    pub source_artifact_uri: ArtifactUri,
    pub source_sha256: veoveo_artifact_contract::UploadSha256,
    pub model: String,
    pub model_revision: String,
    pub transcript: Transcript,
}

pub fn validate_source(metadata: &ArtifactMetadata) -> Result<()> {
    ensure!(
        metadata.byte_len > 0 && metadata.byte_len <= MAX_SOURCE_BYTES,
        "source must be nonempty and at most 2 GiB"
    );
    let mime = metadata
        .mime_type
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default();
    ensure!(
        mime.starts_with("audio/") || mime.starts_with("video/"),
        "source must be audio or video"
    );
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    try_from = "TranscriptionOutputValue",
    into = "TranscriptionOutputValue"
)]
pub struct TranscriptionOutput(veoveo_types::Checked<TranscriptionOutputValue>);
impl std::ops::Deref for TranscriptionOutput {
    type Target = TranscriptionOutputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for TranscriptionOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TranscriptionOutput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        TranscriptionOutputValue::json_schema(generator)
    }
}
impl TryFrom<TranscriptionOutputValue> for TranscriptionOutput {
    type Error = anyhow::Error;
    fn try_from(value: TranscriptionOutputValue) -> Result<Self> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<TranscriptionOutput> for TranscriptionOutputValue {
    fn from(value: TranscriptionOutput) -> Self {
        value.0.into_inner()
    }
}
impl TranscriptionOutputValue {
    pub fn build(self) -> Result<TranscriptionOutput> {
        self.try_into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "TranscriptDocumentValue", into = "TranscriptDocumentValue")]
pub struct TranscriptDocument(veoveo_types::Checked<TranscriptDocumentValue>);
impl std::ops::Deref for TranscriptDocument {
    type Target = TranscriptDocumentValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for TranscriptDocument {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TranscriptDocument".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        TranscriptDocumentValue::json_schema(generator)
    }
}
impl TryFrom<TranscriptDocumentValue> for TranscriptDocument {
    type Error = anyhow::Error;
    fn try_from(value: TranscriptDocumentValue) -> Result<Self> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<TranscriptDocument> for TranscriptDocumentValue {
    fn from(value: TranscriptDocument) -> Self {
        value.0.into_inner()
    }
}
impl TranscriptDocumentValue {
    pub fn build(self) -> Result<TranscriptDocument> {
        self.try_into()
    }
}

impl veoveo_types::Check for TranscriptDocumentValue {
    type Error = anyhow::Error;
    fn check(&self) -> Result<()> {
        ensure!(
            self.schema == TRANSCRIPT_SCHEMA,
            "unsupported Speech transcript format; coordinated upgrade to veoveo.ai/speech-transcript/v2 required"
        );
        self.transcript.validate(transcript::MAX_RECORDING_SECONDS)
    }
}
impl veoveo_types::Check for TranscriptionOutputValue {
    type Error = anyhow::Error;
    fn check(&self) -> Result<()> {
        ensure!(
            self.duration_seconds.is_finite()
                && self.duration_seconds >= 0.0
                && self.duration_seconds <= f64::from(transcript::MAX_RECORDING_SECONDS) + 0.1,
            "invalid transcription duration"
        );
        ensure!(
            self.transcript.artifact_id() != self.captions.artifact_id(),
            "transcription occurrences must be distinct"
        );
        for (metadata, mime) in [
            (&self.transcript, TRANSCRIPT_MIME),
            (&self.captions, "text/vtt"),
        ] {
            ensure!(
                metadata
                    .artifact_uri
                    .as_resource_uri()
                    .components()?
                    .scheme()
                    == "speech"
                    && metadata.download_url.is_none()
                    && metadata.mime_type.as_deref() == Some(mime),
                "invalid transcription Artifact presentation"
            );
        }
        #[derive(Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Attribution {
            source_artifact_uri: ArtifactUri,
            source_sha256: veoveo_artifact_contract::UploadSha256,
            model: String,
            model_revision: String,
        }
        let transcript: Attribution = serde_json::from_value(self.transcript.metadata.clone())?;
        let captions: Attribution = serde_json::from_value(self.captions.metadata.clone())?;
        ensure!(
            transcript == captions && transcript.source_artifact_uri == self.source_artifact_uri,
            "transcription Artifact attribution mismatch"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_network_and_local_source_authority() {
        for uri in [
            "https://example.com/audio.wav",
            "file:///etc/passwd",
            "speech://transcript/anything",
        ] {
            assert!(
                serde_json::from_value::<TranscribeRequest>(
                    serde_json::json!({"artifactUri": uri})
                )
                .is_err()
            );
        }
        let id = ArtifactId::new();
        assert_eq!(
            TranscribeRequest {
                artifact_uri: id.plane_uri()
            }
            .source()
            .unwrap(),
            id
        );
    }
    #[test]
    fn transcription_attribution_refuses_retired_fields_in_each_occurrence() {
        let source = ArtifactId::new();
        let metadata = |mime| {
            let id = ArtifactId::new();
            serde_json::json!({"artifactId":id, "artifactUri":format!("speech://artifact/{id}"),
                "mimeType":mime, "byteLen":1, "createdAt":"2026-09-29T00:00:00Z",
                "metadata":{"sourceArtifactUri":source.plane_uri(), "sourceSha256":"a".repeat(64),
                    "model":"fixture", "modelRevision":"fixture-revision"}})
        };
        let current = serde_json::json!({"resultUri":TranscriptionUri::new(TranscriptionId::new()),
            "sourceArtifactUri":source.plane_uri(), "durationSeconds":1,
            "transcript":metadata("application/json"), "captions":metadata("text/vtt")});
        assert!(serde_json::from_value::<TranscriptionOutput>(current.clone()).is_ok());
        assert!(
            serde_json::from_value::<TranscriptionOutputValue>(current.clone())
                .unwrap()
                .build()
                .is_ok()
        );
        let mut admitted = Vec::new();
        for occurrence in ["transcript", "captions"] {
            for (wire, retired, conflicting) in [
                (
                    "sourceArtifactUri",
                    "source_artifact_uri",
                    serde_json::json!(ArtifactId::new().plane_uri()),
                ),
                (
                    "sourceSha256",
                    "source_sha256",
                    serde_json::json!("b".repeat(64)),
                ),
                (
                    "modelRevision",
                    "model_revision",
                    serde_json::json!("retired-revision"),
                ),
            ] {
                for mode in ["replacement", "mixed", "conflicting"] {
                    let mut invalid = current.clone();
                    let attribution = invalid[occurrence]["metadata"].as_object_mut().unwrap();
                    let original = attribution[wire].clone();
                    attribution.insert(
                        retired.into(),
                        if mode == "conflicting" {
                            conflicting.clone()
                        } else {
                            original
                        },
                    );
                    if mode == "replacement" {
                        attribution.remove(wire);
                    }
                    // Generic Artifact maps admit owner metadata; Speech owns this refusal.
                    assert!(
                        serde_json::from_value::<ArtifactMetadata>(invalid[occurrence].clone())
                            .is_ok()
                    );
                    if serde_json::from_value::<TranscriptionOutput>(invalid.clone()).is_ok() {
                        admitted.push(format!("decoder {occurrence}/{retired}/{mode}"));
                    }
                    if serde_json::from_value::<TranscriptionOutputValue>(invalid)
                        .unwrap()
                        .build()
                        .is_ok()
                    {
                        admitted.push(format!("constructor {occurrence}/{retired}/{mode}"));
                    }
                }
            }
        }
        assert!(
            admitted.is_empty(),
            "retired attribution admitted: {admitted:?}"
        );
    }

    #[test]
    fn current_transcript_format_rejects_retired_tags_and_field_spellings() {
        let schema = schemars::schema_for!(TranscriptDocument);
        let tag =
            schemars::Schema::try_from(schema.as_value()["properties"]["schema"].clone()).unwrap();
        assert_eq!(
            veoveo_types::naming_profile(&tag, veoveo_types::NamingSchemaContext::new(&tag))
                .unwrap()
                .unwrap()
                .role(),
            &veoveo_types::NamingRole::Scalar {
                profile: veoveo_types::ScalarNaming::builtin(
                    veoveo_types::ScalarGrammar::FormatTag
                )
            }
        );
        assert_eq!(tag.as_value()["const"], TRANSCRIPT_SCHEMA);

        let document = TranscriptDocumentValue {
            schema: TRANSCRIPT_SCHEMA.into(),
            source_artifact_uri: ArtifactId::new().plane_uri(),
            source_sha256: veoveo_artifact_contract::UploadSha256::parse(&"a".repeat(64)).unwrap(),
            model: "fixture".into(),
            model_revision: "fixture-revision".into(),
            transcript: transcript::TranscriptValue {
                text: String::new(),
                duration_seconds: 0.0,
                segments: vec![],
            }
            .build()
            .unwrap(),
        }
        .build()
        .unwrap();
        let current = serde_json::to_value(document).unwrap();
        assert!(serde_json::from_value::<TranscriptDocument>(current.clone()).is_ok());
        for tag in [
            "veoveo.speech-transcript/v1",
            "veoveo.ai/speech-transcript/v1",
        ] {
            let mut retired = current.clone();
            retired["schema"] = tag.into();
            let error = serde_json::from_value::<TranscriptDocument>(retired).unwrap_err();
            assert!(error.to_string().contains("coordinated upgrade"));
        }
        for (wire, retired) in [
            ("sourceArtifactUri", "source_artifact_uri"),
            ("sourceSha256", "source_sha256"),
            ("modelRevision", "model_revision"),
        ] {
            for mixed in [false, true] {
                let mut invalid = current.clone();
                invalid[retired] = invalid[wire].clone();
                if !mixed {
                    invalid.as_object_mut().unwrap().remove(wire);
                }
                assert!(serde_json::from_value::<TranscriptDocument>(invalid).is_err());
            }
        }
        let input = TranscribeRequest {
            artifact_uri: ArtifactId::new().plane_uri(),
        };
        let current = serde_json::to_value(input).unwrap();
        assert!(current.get("artifactUri").is_some());
        let mut mixed = current.clone();
        mixed["artifact_uri"] = current["artifactUri"].clone();
        assert!(serde_json::from_value::<TranscribeRequest>(mixed).is_err());
    }
}

fn transcript_format_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    veoveo_types::scalar_schema(
        schemars::Schema::try_from(serde_json::json!({"type":"string","const":TRANSCRIPT_SCHEMA}))
            .expect("current Speech transcript format"),
        veoveo_types::ScalarNaming::builtin(veoveo_types::ScalarGrammar::FormatTag),
    )
    .expect("current Speech transcript naming profile")
}
