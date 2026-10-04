use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct QuantityError {
    name: &'static str,
    value: f64,
    rule: &'static str,
}

impl fmt::Display for QuantityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid {} {}: {}",
            self.name, self.value, self.rule
        )
    }
}

impl std::error::Error for QuantityError {}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct Meters(f64);

impl Meters {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        check_non_negative(value, "meters")?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Meters {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Meters> for f64 {
    fn from(value: Meters) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct MetersPerSecond(f64);

impl MetersPerSecond {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        check_non_negative(value, "meters_per_second")?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for MetersPerSecond {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<MetersPerSecond> for f64 {
    fn from(value: MetersPerSecond) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct Kilograms(f64);

impl Kilograms {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        check_non_negative(value, "kilograms")?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Kilograms {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Kilograms> for f64 {
    fn from(value: Kilograms) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct Seconds(f64);

impl Seconds {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        check_non_negative(value, "seconds")?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Seconds {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Seconds> for f64 {
    fn from(value: Seconds) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct KilowattHours(f64);

impl KilowattHours {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        check_non_negative(value, "kilowatt_hours")?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for KilowattHours {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<KilowattHours> for f64 {
    fn from(value: KilowattHours) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct Liters(f64);

impl Liters {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        check_non_negative(value, "liters")?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Liters {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Liters> for f64 {
    fn from(value: Liters) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct Degrees(f64);

impl Degrees {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        check_non_negative(value, "degrees")?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Degrees {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Degrees> for f64 {
    fn from(value: Degrees) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct Kilopascals(f64);

impl Kilopascals {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        check_non_negative(value, "kilopascals")?;
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Kilopascals {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Kilopascals> for f64 {
    fn from(value: Kilopascals) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
#[schemars(with = "f64")]
pub struct Ratio(f64);

impl Ratio {
    pub fn new(value: f64) -> Result<Self, QuantityError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(QuantityError {
                name: "ratio",
                value,
                rule: "must be finite and within [0, 1]",
            });
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Ratio {
    type Error = QuantityError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Ratio> for f64 {
    fn from(value: Ratio) -> Self {
        value.0
    }
}

fn check_non_negative(value: f64, name: &'static str) -> Result<(), QuantityError> {
    if !value.is_finite() || value < 0.0 {
        return Err(QuantityError {
            name,
            value,
            rule: "must be finite and non-negative",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantities_reject_invalid_values_during_deserialization() {
        assert!(serde_json::from_str::<Meters>("-1").is_err());
        assert!(serde_json::from_str::<Meters>("1.5").is_ok());
        assert!(serde_json::from_str::<Ratio>("1.1").is_err());
    }
}

#[cfg(test)]
mod finite_profile_tests {
    use super::*;
    #[test]
    fn nonnegative_quantities_preserve_zero_and_unbounded_degrees() {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            assert!(Meters::new(invalid).is_err());
            assert!(Degrees::new(invalid).is_err());
        }
        assert!(Meters::new(-0.0).unwrap().get().is_sign_negative());
        assert_eq!(Degrees::new(720.0).unwrap().get(), 720.0);
        assert_eq!(
            Meters::new(-1.0).unwrap_err().to_string(),
            "invalid meters -1: must be finite and non-negative"
        );
    }
}
