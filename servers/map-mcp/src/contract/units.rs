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

macro_rules! non_negative_quantity {
    ($name:ident, $wire:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
        #[serde(try_from = "f64", into = "f64")]
        #[schemars(with = "f64")]
        pub struct $name(f64);

        impl $name {
            pub fn new(value: f64) -> Result<Self, QuantityError> {
                if !value.is_finite() || value < 0.0 {
                    return Err(QuantityError {
                        name: $wire,
                        value,
                        rule: "must be finite and non-negative",
                    });
                }
                Ok(Self(value))
            }

            pub const fn get(self) -> f64 {
                self.0
            }
        }

        impl TryFrom<f64> for $name {
            type Error = QuantityError;

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

non_negative_quantity!(Meters, "meters");
non_negative_quantity!(MetersPerSecond, "meters_per_second");
non_negative_quantity!(Kilograms, "kilograms");
non_negative_quantity!(Seconds, "seconds");
non_negative_quantity!(KilowattHours, "kilowatt_hours");
non_negative_quantity!(Liters, "liters");
non_negative_quantity!(Degrees, "degrees");
non_negative_quantity!(Kilopascals, "kilopascals");

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
