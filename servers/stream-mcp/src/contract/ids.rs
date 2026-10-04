//! Stream-owned identifiers stay distinct through catalog and execution APIs.
use std::fmt;

use serde::{Deserialize, Serialize};
use veoveo_types::TaskId;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum StreamContractError {
    InvalidId(&'static str),
    InvalidResource,
    InvalidCursor,
    InvalidRelationship(&'static str),
}
impl fmt::Display for StreamContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(kind) => write!(f, "invalid Stream {kind} identifier"),
            Self::InvalidResource => f.write_str("invalid Stream resource address"),
            Self::InvalidCursor => f.write_str("invalid Stream collection cursor"),
            Self::InvalidRelationship(field) => write!(f, "inconsistent Stream {field}"),
        }
    }
}
impl std::error::Error for StreamContractError {}

#[veoveo_types::id(text(PipelineIdProfile), error_context = "pipeline")]
pub struct PipelineId(String);

#[veoveo_types::id(text(PipelineIdProfile), error_context = "model")]
pub struct ModelId(String);

/// Identity of a Stream run backed by a native UUIDv7 Task.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
#[veoveo_types::id(custom(error = StreamContractError, admit = admit_run_task, text = |inner: &TaskId| std::borrow::Cow::Owned(inner.to_string()), wire_string, schema = string_schema, schema_inline))]
pub struct RunId(TaskId);
impl RunId {
    pub fn task_id(self) -> TaskId {
        self.0
    }
}
fn admit_run_task(value: &str) -> Result<TaskId, StreamContractError> {
    let task: TaskId = value
        .parse()
        .map_err(|_| StreamContractError::InvalidId("run"))?;
    let id = RunId::try_from(task)?;
    if id.to_string() != value {
        return Err(StreamContractError::InvalidId("run"));
    }
    Ok(id.0)
}

impl TryFrom<TaskId> for RunId {
    type Error = StreamContractError;
    fn try_from(value: TaskId) -> Result<Self, Self::Error> {
        if value.as_uuid().get_version_num() != 7
            || value.as_uuid().get_variant() != uuid::Variant::RFC4122
        {
            return Err(StreamContractError::InvalidId("run"));
        }
        Ok(Self(value))
    }
}
/// Process-local live-session identity, distinct from a recording run's Task.
#[veoveo_types::id(uuid(SessionIds))]
pub struct SessionId(uuid::Uuid);

#[doc(hidden)]
pub struct SessionIds;
impl IdProfile for SessionIds {
    type Error = StreamContractError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        schema: IdSchema::Owner {
            schema: |generator, _| string_schema(generator),
            inline: true,
        },
        ..IdProfileSpec::uuid(
            UuidGrammar {
                versions: &[7],
                variant: UuidVariant::Rfc4122,
                spelling: UuidSpelling::CanonicalLowerHyphenated,
            },
            |_, _, _| StreamContractError::InvalidId("session"),
        )
    };
}

use veoveo_types::{IdProfile, IdProfileSpec, IdSchema, UuidGrammar, UuidSpelling, UuidVariant};

#[doc(hidden)]
pub struct PipelineIdProfile;
impl IdProfile for PipelineIdProfile {
    type Error = StreamContractError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        schema: IdSchema::Owner {
            schema: |generator, _| string_schema(generator),
            inline: true,
        },
        ..IdProfileSpec::text(|value, metadata| validate_catalog_id(value, metadata.error_context))
    };
}

fn string_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <String as schemars::JsonSchema>::json_schema(generator)
}

fn validate_catalog_id(value: &str, kind: &'static str) -> Result<(), StreamContractError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        || !value.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(StreamContractError::InvalidId(kind));
    }
    Ok(())
}
