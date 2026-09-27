use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn validate_coordinate_id(value: &str) -> Result<(), GeodeticIdError> {
    if value.is_empty() || value.len() > 128 {
        return Err(GeodeticIdError::new(value, "must be 1 to 128 characters"));
    }
    if value.contains("://") || value.contains('/') || value.chars().any(char::is_whitespace) {
        return Err(GeodeticIdError::new(
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
        Err(GeodeticIdError::new(
            value,
            "must contain only ASCII letters, digits, underscore, dash, dot, or colon",
        ))
    }
}

macro_rules! coordinate_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, GeodeticIdError> {
                let value = value.into();
                validate_coordinate_id(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = GeodeticIdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = GeodeticIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value.to_string())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeodeticIdError {
    value: String,
    rule: &'static str,
}

impl GeodeticIdError {
    pub(super) fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for GeodeticIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid coordinate identifier {:?}: {}",
            self.value, self.rule
        )
    }
}

impl std::error::Error for GeodeticIdError {}

coordinate_id!(CrsId, "Coordinate reference system id, commonly EPSG:4326.");
coordinate_id!(
    DatumId,
    "Geodetic datum id used by a CRS or coordinate operation."
);
coordinate_id!(
    EllipsoidId,
    "Reference ellipsoid id, such as WGS84 or GRS80."
);
