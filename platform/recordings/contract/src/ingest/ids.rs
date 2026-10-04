//! Recording producer transport identities and their existing wire admission profiles.

use veoveo_types::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id},
};

/// Configured identity of one governed recording producer.
#[veoveo_types::id(text(PathIdProfile))]
pub struct RecordingProducerId(String);
/// Installation-owned dataset name assigned to a recording producer.
#[veoveo_types::id(text(DatasetNameProfile))]
pub struct RecordingDatasetName(String);
/// Rerun application id admitted for a recording producer.
#[veoveo_types::id(text(ClaimTextProfile))]
pub struct RecordingApplicationId(String);
/// Canonical UUIDv7 identity of one authenticated recording ingest stream.
#[veoveo_types::id(uuid(StreamIdProfile))]
pub struct RecordingIngestStreamId(String);
use veoveo_types::{IdProfile, IdProfileSpec, UuidGrammar, UuidSpelling, UuidVariant};

#[doc(hidden)]
pub struct PathIdProfile;
impl IdProfile for PathIdProfile {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        schema_id: Some(ingest_schema_id),
        ..IdProfileSpec::text(|value, _| validate_path_id(value))
    };
}
#[doc(hidden)]
pub struct DatasetNameProfile;
impl IdProfile for DatasetNameProfile {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        schema_id: Some(ingest_schema_id),
        ..IdProfileSpec::text(|value, _| validate_dataset_name(value))
    };
}
#[doc(hidden)]
pub struct ClaimTextProfile;
impl IdProfile for ClaimTextProfile {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        schema_id: Some(ingest_schema_id),
        ..IdProfileSpec::text(|value, _| validate_claim_text(value))
    };
}
#[doc(hidden)]
pub struct StreamIdProfile;
impl IdProfile for StreamIdProfile {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        schema_id: Some(ingest_schema_id),
        ..IdProfileSpec::uuid(
            UuidGrammar {
                versions: &[7],
                variant: UuidVariant::Any,
                spelling: UuidSpelling::ParserAliases,
            },
            |value, _, _| IdentifierError::new(value, "must be a UUIDv7"),
        )
    };
}

fn validate_dataset_name(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

fn ingest_schema_id(metadata: veoveo_types::IdMetadata) -> std::borrow::Cow<'static, str> {
    format!("veoveo_mcp_contract::gateway::ids::{}", metadata.type_name).into()
}
