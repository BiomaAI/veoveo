use std::{fmt, num::NonZeroU32};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::parse_artifact_plane_uri;

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum OptimizationContractError {
    #[error("invalid {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid {0} URI")]
    InvalidUri(&'static str),
    #[error("{0} must be finite and {1}")]
    InvalidNumber(&'static str, &'static str),
    #[error("{field} must contain between {minimum} and {maximum} entries")]
    InvalidCollection {
        field: &'static str,
        minimum: usize,
        maximum: usize,
    },
    #[error("{0}")]
    InvalidProblem(String),
}

macro_rules! controlled_id {
    ($name:ident, $label:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, OptimizationContractError> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 128
                    || value.trim() != value
                    || value.chars().any(|character| {
                        !(character.is_ascii_alphanumeric()
                            || matches!(character, '-' | '_' | '.' | ':'))
                    })
                {
                    return Err(OptimizationContractError::InvalidIdentifier($label));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = OptimizationContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

macro_rules! output_id {
    ($name:ident, $prefix:literal, $label:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(format!("{}{}", $prefix, uuid::Uuid::now_v7()))
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, OptimizationContractError> {
                let value = value.into();
                let parsed = value
                    .strip_prefix($prefix)
                    .and_then(|suffix| uuid::Uuid::parse_str(suffix).ok())
                    .filter(|uuid| uuid.get_version_num() == 7)
                    .ok_or(OptimizationContractError::InvalidIdentifier($label))?;
                Ok(Self(format!("{}{}", $prefix, parsed)))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = OptimizationContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> String {
                value.0
            }
        }
    };
}

controlled_id!(LocationId, "location id");
controlled_id!(OrderId, "order id");
controlled_id!(VehicleId, "vehicle id");
controlled_id!(VehicleTypeId, "vehicle type id");
controlled_id!(CapacityDimensionId, "capacity dimension id");
controlled_id!(RouteCaseId, "route case id");
controlled_id!(VariableId, "variable id");
controlled_id!(ConstraintId, "constraint id");
controlled_id!(SolverProfileId, "solver profile id");

output_id!(ProblemId, "problem-", "problem id");
output_id!(RunId, "run-", "run id");
output_id!(SolutionId, "solution-", "solution id");
output_id!(VerificationId, "verification-", "verification id");

macro_rules! uri_type {
    ($name:ident, $label:literal, $validator:expr) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, OptimizationContractError> {
                let value = value.into();
                if ($validator)(&value) {
                    Ok(Self(value))
                } else {
                    Err(OptimizationContractError::InvalidUri($label))
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = OptimizationContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> String {
                value.0
            }
        }
    };
}

fn valid_uri_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains(['/', '?', '#'])
        && !value.chars().any(char::is_control)
}

uri_type!(ArtifactUri, "artifact", |value: &str| {
    parse_artifact_plane_uri(value).is_some()
});
uri_type!(MapTravelModelUri, "Map travel-model", |value: &str| {
    value
        .strip_prefix("map://travel-model/")
        .is_some_and(valid_uri_segment)
});
uri_type!(
    OptimizationProblemUri,
    "Optimization problem",
    |value: &str| {
        value
            .strip_prefix("optimization://problem/")
            .is_some_and(valid_uri_segment)
    }
);
uri_type!(
    OptimizationSolutionUri,
    "Optimization solution",
    |value: &str| {
        value
            .strip_prefix("optimization://solution/")
            .is_some_and(valid_uri_segment)
    }
);
uri_type!(OptimizationRunUri, "Optimization run", |value: &str| {
    value
        .strip_prefix("optimization://run/")
        .is_some_and(valid_uri_segment)
});
uri_type!(
    OptimizationProfileUri,
    "Optimization profile",
    |value: &str| {
        value
            .strip_prefix("optimization://profile/")
            .is_some_and(valid_uri_segment)
    }
);

macro_rules! finite_number {
    ($name:ident, $label:literal, $requirement:literal, $predicate:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
        #[serde(try_from = "f64", into = "f64")]
        #[schemars(with = "f64")]
        pub struct $name(f64);

        impl $name {
            pub fn new(value: f64) -> Result<Self, OptimizationContractError> {
                if !value.is_finite() || !($predicate)(value) {
                    return Err(OptimizationContractError::InvalidNumber(
                        $label,
                        $requirement,
                    ));
                }
                Ok(Self(value))
            }

            pub const fn get(self) -> f64 {
                self.0
            }
        }

        impl TryFrom<f64> for $name {
            type Error = OptimizationContractError;

            fn try_from(value: f64) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for f64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

finite_number!(
    FiniteF64,
    "finite value",
    "representable as a finite f64",
    |_value: f64| true
);
finite_number!(
    NonNegativeF64,
    "non-negative value",
    "greater than or equal to zero",
    |value: f64| value >= 0.0
);
finite_number!(
    PositiveF64,
    "positive value",
    "greater than zero",
    |value: f64| value > 0.0
);
finite_number!(
    UnitInterval,
    "unit interval",
    "within zero and one inclusive",
    |value: f64| (0.0..=1.0).contains(&value)
);

impl Default for FiniteF64 {
    fn default() -> Self {
        Self::new(0.0).expect("zero is finite")
    }
}

impl Default for NonNegativeF64 {
    fn default() -> Self {
        Self::new(0.0).expect("zero is non-negative")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    Second,
    Minute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeBasis {
    pub origin: DateTime<Utc>,
    pub unit: TimeUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeWindow {
    pub earliest: u32,
    pub latest: u32,
}

impl TimeWindow {
    pub fn validate(self, field: &'static str) -> Result<(), OptimizationContractError> {
        if self.earliest > self.latest {
            return Err(OptimizationContractError::InvalidProblem(format!(
                "{field} earliest must not exceed latest"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SolverPolicyRef {
    pub profile_uri: OptimizationProfileUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_seconds: Option<NonZeroU32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_target: Option<QualityTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QualityTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_gap: Option<UnitInterval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absolute_gap: Option<NonNegativeF64>,
}

pub(crate) fn require_collection(
    field: &'static str,
    actual: usize,
    minimum: usize,
    maximum: usize,
) -> Result<(), OptimizationContractError> {
    if !(minimum..=maximum).contains(&actual) {
        return Err(OptimizationContractError::InvalidCollection {
            field,
            minimum,
            maximum,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_ids_reject_path_segments() {
        assert!(LocationId::new("warehouse-1").is_ok());
        assert!(LocationId::new("../warehouse").is_err());
    }

    #[test]
    fn output_ids_are_uuid_v7() {
        let id = ProblemId::new();
        assert_eq!(ProblemId::parse(id.to_string()).unwrap(), id);
    }

    #[test]
    fn resource_uris_are_family_specific() {
        assert!(MapTravelModelUri::parse("map://travel-model/travel-1").is_ok());
        assert!(MapTravelModelUri::parse("map://matrix/matrix-1").is_err());
        assert!(OptimizationSolutionUri::parse("optimization://solution/solution-1").is_ok());
    }
}
