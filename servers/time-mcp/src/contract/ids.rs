use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! public_id {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if value.len() < $prefix.len() + 1
                    || value.len() > 128
                    || !value.starts_with($prefix)
                    || !value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                    })
                {
                    return Err(format!("expected {} identifier", $prefix));
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

        impl FromStr for $name {
            type Err = String;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

public_id!(TimeSourceId, "time-source-");
public_id!(AuthorityReleaseId, "time-release-");
public_id!(TimeAcquisitionId, "time-acquisition-");
public_id!(CalendarId, "calendar-");
public_id!(MissionEpochId, "epoch-");
public_id!(TemporalEventId, "event-");
