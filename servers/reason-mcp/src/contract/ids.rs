//! Reason-owned identifiers stay distinct through catalog and execution APIs.
use std::fmt;

use serde::{Deserialize, Serialize};
use veoveo_types::TaskId;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ReasonContractError {
    InvalidId(&'static str),
    InvalidResource,
    InvalidCursor,
    InvalidRelationship(&'static str),
}
impl fmt::Display for ReasonContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(kind) => write!(f, "invalid Reason {kind} identifier"),
            Self::InvalidResource => f.write_str("invalid Reason resource address"),
            Self::InvalidCursor => f.write_str("invalid Reason analyses cursor"),
            Self::InvalidRelationship(field) => write!(f, "inconsistent Reason {field}"),
        }
    }
}
impl std::error::Error for ReasonContractError {}

#[veoveo_types::id(text(PipelineIdProfile), error_context = "pipeline")]
pub struct PipelineId(String);

#[veoveo_types::id(text(PipelineIdProfile), error_context = "model")]
pub struct ModelId(String);

/// Identity of a Reason analysis backed by a native UUIDv7 Task.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
#[veoveo_types::id(custom(error = ReasonContractError, admit = admit_analysis_task, text = |inner: &TaskId| std::borrow::Cow::Owned(inner.to_string()), wire_string, schema = string_schema, schema_inline))]
pub struct AnalysisId(TaskId);
impl AnalysisId {
    pub fn task_id(self) -> TaskId {
        self.0
    }
}
fn admit_analysis_task(value: &str) -> Result<TaskId, ReasonContractError> {
    let task: TaskId = value
        .parse()
        .map_err(|_| ReasonContractError::InvalidId("analysis"))?;
    let id = AnalysisId::try_from(task)?;
    if id.to_string() != value {
        return Err(ReasonContractError::InvalidId("analysis"));
    }
    Ok(id.0)
}

impl TryFrom<TaskId> for AnalysisId {
    type Error = ReasonContractError;
    fn try_from(value: TaskId) -> Result<Self, Self::Error> {
        if value.as_uuid().get_version_num() != 7
            || value.as_uuid().get_variant() != uuid::Variant::RFC4122
        {
            return Err(ReasonContractError::InvalidId("analysis"));
        }
        Ok(Self(value))
    }
}
use veoveo_types::{IdProfile, IdProfileSpec};

#[doc(hidden)]
pub struct PipelineIdProfile;
impl IdProfile for PipelineIdProfile {
    type Error = ReasonContractError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, metadata| validate_catalog_id(value, metadata.error_context))
            .owner_schema(|generator, _| string_schema(generator), true);
}

fn string_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <String as schemars::JsonSchema>::json_schema(generator)
}

fn validate_catalog_id(value: &str, kind: &'static str) -> Result<(), ReasonContractError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        || !value.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(ReasonContractError::InvalidId(kind));
    }
    Ok(())
}
