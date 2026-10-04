use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn validate_coordinate_id(value: &str) -> Result<(), GeofenceIdError> {
    if value.is_empty() || value.len() > 128 {
        return Err(GeofenceIdError::new(value, "must be 1 to 128 characters"));
    }
    if value.contains("://") || value.contains('/') || value.chars().any(char::is_whitespace) {
        return Err(GeofenceIdError::new(
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
        Err(GeofenceIdError::new(
            value,
            "must contain only ASCII letters, digits, underscore, dash, dot, or colon",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeofenceIdError {
    value: String,
    rule: &'static str,
}

impl GeofenceIdError {
    pub(super) fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for GeofenceIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid coordinate identifier {:?}: {}",
            self.value, self.rule
        )
    }
}

impl std::error::Error for GeofenceIdError {}

#[doc = "Geofence identity used by validation and plans."]
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
#[id(string, error = GeofenceIdError, validate = validate_coordinate_id)]
pub struct GeofenceId(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    Wgs84,
    Ecef,
    Enu,
    Ned,
    Frd,
    ProjectedCrs,
    SimulationWorld,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeofenceRule {
    MustStayInside,
    MustStayOutside,
}
