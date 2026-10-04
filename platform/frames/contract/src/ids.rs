use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn validate_coordinate_id(value: &str) -> Result<(), FrameIdError> {
    if matches!(value, "." | "..") {
        return Err(FrameIdError::new(
            value,
            "must not be a relative path component",
        ));
    }
    if value.is_empty() || value.len() > 128 {
        return Err(FrameIdError::new(value, "must be 1 to 128 characters"));
    }
    if value.contains("://") || value.contains('/') || value.chars().any(char::is_whitespace) {
        return Err(FrameIdError::new(
            value,
            "must not contain whitespace, slash, or URI separators",
        ));
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
    {
        Ok(())
    } else {
        Err(FrameIdError::new(
            value,
            "must contain only ASCII letters, digits, underscore, dash, dot, or colon",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameIdError {
    value: String,
    rule: &'static str,
}

impl FrameIdError {
    pub(super) fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for FrameIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid coordinate identifier {:?}: {}",
            self.value, self.rule
        )
    }
}

impl std::error::Error for FrameIdError {}

#[doc = "Coordinate operation frame id for solver inputs, resources, and provenance. RRD transform frame ids live in veoveo-rrd."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = FrameIdError, validate = validate_coordinate_id)]
pub struct FrameId(String);
#[doc = "Stable identity of one authored coordinate-frame world."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = FrameIdError, validate = validate_coordinate_id)]
pub struct FrameWorldId(String);
#[doc = "Immutable identity of one complete coordinate-frame world revision."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = FrameIdError, validate = validate_coordinate_id)]
pub struct FrameWorldRevisionId(String);
#[doc = "Durable id for one coordinate transform, projection, geodesic, or validation operation."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = FrameIdError, validate = validate_coordinate_id)]
pub struct CoordinateOperationId(String);
