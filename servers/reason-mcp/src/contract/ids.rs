//! Reason-owned identifiers stay distinct through catalog and execution APIs.
use std::{fmt, str::FromStr};

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

#[derive(
    veoveo_types::Id, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=ReasonContractError,validate=|value| validate_catalog_id(value,"pipeline"),schema=String::json_schema,schema_inline)]
pub struct PipelineId(String);

#[derive(
    veoveo_types::Id, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=ReasonContractError,validate=|value| validate_catalog_id(value,"model"),schema=String::json_schema,schema_inline)]
pub struct ModelId(String);

/// Identity of a Reason analysis backed by a native UUIDv7 Task.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AnalysisId(TaskId);
impl AnalysisId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ReasonContractError> {
        let value = value.as_ref();
        let task: TaskId = value
            .parse()
            .map_err(|_| ReasonContractError::InvalidId("analysis"))?;
        let id = Self::try_from(task)?;
        if id.to_string() != value {
            return Err(ReasonContractError::InvalidId("analysis"));
        }
        Ok(id)
    }
    pub fn task_id(self) -> TaskId {
        self.0
    }
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
impl TryFrom<String> for AnalysisId {
    type Error = ReasonContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl FromStr for AnalysisId {
    type Err = ReasonContractError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl From<AnalysisId> for String {
    fn from(value: AnalysisId) -> Self {
        value.to_string()
    }
}
impl veoveo_types::Identity for AnalysisId {
    type Error = ReasonContractError;
    fn parse_identity(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
    fn identity_text(&self) -> std::borrow::Cow<'_, str> {
        self.0.to_string().into()
    }
}
impl fmt::Display for AnalysisId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl schemars::JsonSchema for AnalysisId {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AnalysisId".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
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
