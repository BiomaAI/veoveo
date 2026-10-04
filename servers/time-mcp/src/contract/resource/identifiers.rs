use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::TimeResourceError;

/// A relative TZDB key. The active authority establishes whether the zone exists.
#[veoveo_types::id(text(TimeZones))]
pub struct TimeZoneId(String);

impl TimeZoneId {
    pub(super) fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

#[doc(hidden)]
pub struct TimeZones;
impl veoveo_types::IdProfile for TimeZones {
    type Error = TimeResourceError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| {
            if value.len() > 1024
                || value.split('/').any(|part| {
                    part.is_empty()
                        || matches!(part, "." | "..")
                        || !part.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric()
                                || matches!(byte, b'_' | b'-' | b'+' | b'.')
                        })
                })
            {
                Err(TimeResourceError::InvalidZone)
            } else {
                Ok(())
            }
        });
}

/// The documents embedded by this server's hosted surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeDocument {
    Agents,
    Design,
}

impl TimeDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }

    pub fn parse(value: &str) -> Result<Self, TimeResourceError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(TimeResourceError::UnknownResource),
        }
    }
}
