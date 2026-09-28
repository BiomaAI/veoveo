//! The existing v1 cursor wire format, with collection-specific public types.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::{TimeResourceError, TimeVersion};
use crate::{
    contract::{CalendarId, MissionEpochId, SubsecondNanoseconds, TemporalEventId},
    uris,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionPosition<I> {
    key: I,
    version: TimeVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventPosition {
    tai_seconds: i64,
    nanosecond: SubsecondNanoseconds,
    event_key: TemporalEventId,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCursor<P> {
    version: u8,
    collection: String,
    position: P,
}

fn decode<P: DeserializeOwned>(wire: &str, collection: &str) -> Result<P, TimeResourceError> {
    if wire.is_empty() || wire.len() > 2048 {
        return Err(TimeResourceError::InvalidCursor);
    }
    let bytes = hex::decode(wire).map_err(|_| TimeResourceError::InvalidCursor)?;
    let cursor: WireCursor<P> =
        serde_json::from_slice(&bytes).map_err(|_| TimeResourceError::InvalidCursor)?;
    if cursor.version != 1 || cursor.collection != collection {
        return Err(TimeResourceError::InvalidCursor);
    }
    Ok(cursor.position)
}

macro_rules! cursor_type {
    ($name:ident, $position:ty, $root:path) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name {
            wire: String,
            position: $position,
        }

        impl $name {
            fn from_position(position: $position) -> Self {
                let wire = hex::encode(
                    serde_json::to_vec(&WireCursor {
                        version: 1,
                        collection: $root.to_owned(),
                        position: &position,
                    })
                    .expect("closed Time cursor fields serialize"),
                );
                Self { wire, position }
            }

            pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
                let wire = wire.into();
                let position = decode(&wire, $root)?;
                Ok(Self { wire, position })
            }

            pub fn as_str(&self) -> &str {
                &self.wire
            }
        }

        impl TryFrom<String> for $name {
            type Error = TimeResourceError;
            fn try_from(wire: String) -> Result<Self, Self::Error> {
                Self::parse(wire)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.wire
            }
        }
    };
}

cursor_type!(
    CalendarCursor,
    VersionPosition<CalendarId>,
    uris::CALENDARS_URI
);
cursor_type!(
    EpochCursor,
    VersionPosition<MissionEpochId>,
    uris::EPOCHS_URI
);
cursor_type!(EventCursor, EventPosition, uris::EVENTS_URI);

impl CalendarCursor {
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::{CalendarCursor, MissionEpochId, TimeVersion};
    /// CalendarCursor::new(&MissionEpochId::new("epoch-example").unwrap(), TimeVersion::new(1).unwrap());
    /// ```
    pub fn new(key: &CalendarId, version: TimeVersion) -> Self {
        Self::from_position(VersionPosition {
            key: key.clone(),
            version,
        })
    }
    pub fn calendar_id(&self) -> &CalendarId {
        &self.position.key
    }
    pub fn version(&self) -> TimeVersion {
        self.position.version
    }
}

impl EpochCursor {
    pub fn new(key: &MissionEpochId, version: TimeVersion) -> Self {
        Self::from_position(VersionPosition {
            key: key.clone(),
            version,
        })
    }
    pub fn epoch_id(&self) -> &MissionEpochId {
        &self.position.key
    }
    pub fn version(&self) -> TimeVersion {
        self.position.version
    }
}

impl EventCursor {
    /// ```compile_fail
    /// use veoveo_time_mcp::{EventCursor, TemporalEventId};
    /// EventCursor::new(&TemporalEventId::new("event-example").unwrap(), 0, 1_000_000_000);
    /// ```
    pub fn new(key: &TemporalEventId, tai_seconds: i64, nanosecond: SubsecondNanoseconds) -> Self {
        let position = EventPosition {
            event_key: key.clone(),
            tai_seconds,
            nanosecond,
        };
        Self::from_position(position)
    }
    pub fn event_id(&self) -> &TemporalEventId {
        &self.position.event_key
    }
    pub fn tai_seconds(&self) -> i64 {
        self.position.tai_seconds
    }
    pub fn nanosecond(&self) -> SubsecondNanoseconds {
        self.position.nanosecond
    }
}
