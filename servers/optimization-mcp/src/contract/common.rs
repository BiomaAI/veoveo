use super::OptimizationProfileUri;
use std::num::NonZeroU32;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"location id",|_: &str| true))]
pub struct LocationId(String);

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"order id",|_: &str| true))]
pub struct OrderId(String);

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"vehicle id",|_: &str| true))]
pub struct VehicleId(String);

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"vehicle type id",|_: &str| true))]
pub struct VehicleTypeId(String);

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"capacity dimension id",|_: &str| true))]
pub struct CapacityDimensionId(String);

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"route case id",|_: &str| true))]
pub struct RouteCaseId(String);

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"variable id",|_: &str| true))]
pub struct VariableId(String);

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"constraint id",|_: &str| true))]
pub struct ConstraintId(String);

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
#[schemars(with = "String")]
#[id(string,error=OptimizationContractError,validate=|value| validate_controlled_id(value,"solver profile id",|value: &str| {
    !matches!(value, "." | "..") && !value.contains(':')
}))]
pub struct SolverProfileId(String);

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
#[schemars(with = "String")]
#[id(string,constructor=parse,error=OptimizationContractError,validate=|value| validate_output_id(value,"problem-","problem id"),generate=|| format!("{}{}","problem-",uuid::Uuid::now_v7()))]
pub struct ProblemId(String);

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
#[schemars(with = "String")]
#[id(string,constructor=parse,error=OptimizationContractError,validate=|value| validate_output_id(value,"run-","run id"),generate=|| format!("{}{}","run-",uuid::Uuid::now_v7()))]
pub struct RunId(String);

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
#[schemars(with = "String")]
#[id(string,constructor=parse,error=OptimizationContractError,validate=|value| validate_output_id(value,"solution-","solution id"),generate=|| format!("{}{}","solution-",uuid::Uuid::now_v7()))]
pub struct SolutionId(String);

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
#[schemars(with = "String")]
#[id(string,constructor=parse,error=OptimizationContractError,validate=|value| validate_output_id(value,"verification-","verification id"),generate=|| format!("{}{}","verification-",uuid::Uuid::now_v7()))]
pub struct VerificationId(String);

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

fn validate_controlled_id(
    value: &str,
    label: &'static str,
    admit: impl FnOnce(&str) -> bool,
) -> Result<(), OptimizationContractError> {
    if !admit(value)
        || value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')))
    {
        return Err(OptimizationContractError::InvalidIdentifier(label));
    }
    Ok(())
}
fn validate_output_id(
    value: &str,
    prefix: &str,
    label: &'static str,
) -> Result<(), OptimizationContractError> {
    let parsed = value
        .strip_prefix(prefix)
        .and_then(|suffix| uuid::Uuid::parse_str(suffix).ok())
        .filter(|uuid| uuid.get_version_num() == 7 && uuid.get_variant() == uuid::Variant::RFC4122)
        .ok_or(OptimizationContractError::InvalidIdentifier(label))?;
    if value != format!("{}{}", prefix, parsed) {
        return Err(OptimizationContractError::InvalidIdentifier(label));
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
}
