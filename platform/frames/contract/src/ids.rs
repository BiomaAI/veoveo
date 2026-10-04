use std::fmt;

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
#[veoveo_types::id(text(CoordinateIds))]
pub struct FrameId(String);
#[doc = "Stable identity of one authored coordinate-frame world."]
#[veoveo_types::id(text(CoordinateIds))]
pub struct FrameWorldId(String);
#[doc = "Immutable identity of one complete coordinate-frame world revision."]
#[veoveo_types::id(text(CoordinateIds))]
pub struct FrameWorldRevisionId(String);
#[doc = "Durable id for one coordinate transform, projection, geodesic, or validation operation."]
#[veoveo_types::id(text(CoordinateIds))]
pub struct CoordinateOperationId(String);

use veoveo_types::{IdProfile, IdProfileSpec};

#[doc(hidden)]
pub struct CoordinateIds;
impl IdProfile for CoordinateIds {
    type Error = FrameIdError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_coordinate_id(value));
}
