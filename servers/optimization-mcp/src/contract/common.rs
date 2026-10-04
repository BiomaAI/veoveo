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

#[veoveo_types::id(text(LocationIdProfile), error_context = "location id")]
#[schemars(with = "String")]
pub struct LocationId(String);

#[veoveo_types::id(text(LocationIdProfile), error_context = "order id")]
#[schemars(with = "String")]
pub struct OrderId(String);

#[veoveo_types::id(text(LocationIdProfile), error_context = "vehicle id")]
#[schemars(with = "String")]
pub struct VehicleId(String);

#[veoveo_types::id(text(LocationIdProfile), error_context = "vehicle type id")]
#[schemars(with = "String")]
pub struct VehicleTypeId(String);

#[veoveo_types::id(text(LocationIdProfile), error_context = "capacity dimension id")]
#[schemars(with = "String")]
pub struct CapacityDimensionId(String);

#[veoveo_types::id(text(LocationIdProfile), error_context = "route case id")]
#[schemars(with = "String")]
pub struct RouteCaseId(String);

#[veoveo_types::id(text(LocationIdProfile), error_context = "variable id")]
#[schemars(with = "String")]
pub struct VariableId(String);

#[veoveo_types::id(text(LocationIdProfile), error_context = "constraint id")]
#[schemars(with = "String")]
pub struct ConstraintId(String);

#[veoveo_types::id(text(SolverProfileIdProfile))]
#[schemars(with = "String")]
pub struct SolverProfileId(String);

#[veoveo_types::id(
    prefixed(ProblemIdProfile, "problem-"),
    fresh,
    error_context = "problem id"
)]
#[schemars(with = "String")]
pub struct ProblemId(String);

#[veoveo_types::id(prefixed(ProblemIdProfile, "run-"), fresh, error_context = "run id")]
#[schemars(with = "String")]
pub struct RunId(String);

#[veoveo_types::id(
    prefixed(ProblemIdProfile, "solution-"),
    fresh,
    error_context = "solution id"
)]
#[schemars(with = "String")]
pub struct SolutionId(String);

#[veoveo_types::id(
    prefixed(ProblemIdProfile, "verification-"),
    fresh,
    error_context = "verification id"
)]
#[schemars(with = "String")]
pub struct VerificationId(String);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct FiniteF64(f64);

impl FiniteF64 {
    pub fn new(value: f64) -> Result<Self, OptimizationContractError> {
        check_number(
            value,
            "finite value",
            "representable as a finite f64",
            |_value: f64| true,
        )?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for FiniteF64 {
    type Error = OptimizationContractError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<FiniteF64> for f64 {
    fn from(value: FiniteF64) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct NonNegativeF64(f64);

impl NonNegativeF64 {
    pub fn new(value: f64) -> Result<Self, OptimizationContractError> {
        check_number(
            value,
            "non-negative value",
            "greater than or equal to zero",
            |value: f64| value >= 0.0,
        )?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for NonNegativeF64 {
    type Error = OptimizationContractError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<NonNegativeF64> for f64 {
    fn from(value: NonNegativeF64) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct PositiveF64(f64);

impl PositiveF64 {
    pub fn new(value: f64) -> Result<Self, OptimizationContractError> {
        check_number(
            value,
            "positive value",
            "greater than zero",
            |value: f64| value > 0.0,
        )?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for PositiveF64 {
    type Error = OptimizationContractError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PositiveF64> for f64 {
    fn from(value: PositiveF64) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct UnitInterval(f64);

impl UnitInterval {
    pub fn new(value: f64) -> Result<Self, OptimizationContractError> {
        check_number(
            value,
            "unit interval",
            "within zero and one inclusive",
            |value: f64| (0.0..=1.0).contains(&value),
        )?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for UnitInterval {
    type Error = OptimizationContractError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<UnitInterval> for f64 {
    fn from(value: UnitInterval) -> Self {
        value.0
    }
}

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
#[serde(deny_unknown_fields)]
pub struct TimeBasis {
    pub origin: DateTime<Utc>,
    pub unit: TimeUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct SolverPolicyRef {
    pub profile_uri: OptimizationProfileUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_seconds: Option<NonZeroU32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_target: Option<QualityTarget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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

fn check_number(
    value: f64,
    label: &'static str,
    requirement: &'static str,
    predicate: impl FnOnce(f64) -> bool,
) -> Result<(), OptimizationContractError> {
    if !value.is_finite() || !predicate(value) {
        return Err(OptimizationContractError::InvalidNumber(label, requirement));
    }
    Ok(())
}

use veoveo_types::{IdProfile, IdProfileSpec, UuidGrammar};

#[doc(hidden)]
pub struct LocationIdProfile;
impl IdProfile for LocationIdProfile {
    type Error = OptimizationContractError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, metadata| {
        validate_controlled_id(value, metadata.error_context, |_: &str| true)
    });
}

#[doc(hidden)]
pub struct SolverProfileIdProfile;
impl IdProfile for SolverProfileIdProfile {
    type Error = OptimizationContractError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
        validate_controlled_id(value, "solver profile id", |value: &str| {
            !matches!(value, "." | "..") && !value.contains(':')
        })
    });
}
#[doc(hidden)]
pub struct ProblemIdProfile;
impl IdProfile for ProblemIdProfile {
    type Error = OptimizationContractError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::generated_uuid(UuidGrammar::canonical(&[7]), |_, metadata, _| {
            OptimizationContractError::InvalidIdentifier(metadata.error_context)
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_ids_reject_path_segments() {
        assert!(LocationId::parse("warehouse-1").is_ok());
        assert!(LocationId::parse("../warehouse").is_err());
    }

    #[test]
    fn output_ids_are_uuid_v7() {
        let id = ProblemId::new();
        assert_eq!(ProblemId::parse(&id).unwrap(), id);
    }
}

#[cfg(test)]
mod numeric_profile_tests {
    use super::*;
    #[test]
    fn finite_owner_policies_and_negative_zero_are_preserved() {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(FiniteF64::new(invalid).is_err());
            assert!(NonNegativeF64::new(invalid).is_err());
            assert!(PositiveF64::new(invalid).is_err());
            assert!(UnitInterval::new(invalid).is_err());
        }
        assert!(FiniteF64::new(-1.0).is_ok());
        assert!(NonNegativeF64::new(-1.0).is_err());
        assert!(PositiveF64::new(0.0).is_err());
        assert!(UnitInterval::new(1.0).is_ok());
        assert!(UnitInterval::new(1.1).is_err());
        assert!(NonNegativeF64::new(-0.0).unwrap().get().is_sign_negative());
        assert_eq!(
            PositiveF64::new(0.0).unwrap_err().to_string(),
            "positive value must be finite and greater than zero"
        );
    }
}
