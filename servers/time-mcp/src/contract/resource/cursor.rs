//! The existing v1 cursor wire format, with collection-specific public types.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::{TimeResourceError, TimeVersion};
use crate::{
    contract::{
        AuthorityReleaseId, CalendarId, MissionEpochId, SubsecondNanoseconds, TemporalEventId,
    },
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct TimeCursorCodec<P> {
    collection: &'static str,
    marker: std::marker::PhantomData<P>,
}
impl<P> TimeCursorCodec<P> {
    fn new(collection: &'static str) -> Self {
        Self {
            collection,
            marker: std::marker::PhantomData,
        }
    }
}
impl<P: Serialize + DeserializeOwned> veoveo_types::CursorCodec for TimeCursorCodec<P> {
    type Position = P;
    type Error = TimeResourceError;
    fn check(&self, _position: &P) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &P) -> Result<String, Self::Error> {
        Ok(hex::encode(
            serde_json::to_vec(&WireCursor {
                version: 1,
                collection: self.collection.to_owned(),
                position,
            })
            .expect("closed Time cursor fields serialize"),
        ))
    }
    fn decode(&self, wire: &str) -> Result<P, Self::Error> {
        decode(wire, self.collection)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct CalendarCursor {
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<VersionPosition<CalendarId>>>,
}
impl CalendarCursor {
    fn from_position(position: VersionPosition<CalendarId>) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(
                TimeCursorCodec::new(uris::CALENDARS_URI),
                position,
            )
            .expect("typed Time cursor"),
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(TimeCursorCodec::new(uris::CALENDARS_URI), wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
impl TryFrom<String> for CalendarCursor {
    type Error = TimeResourceError;
    fn try_from(wire: String) -> Result<Self, Self::Error> {
        Self::parse(wire)
    }
}
impl From<CalendarCursor> for String {
    fn from(value: CalendarCursor) -> Self {
        value.cursor.into_wire()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct EpochCursor {
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<VersionPosition<MissionEpochId>>>,
}
impl EpochCursor {
    fn from_position(position: VersionPosition<MissionEpochId>) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(
                TimeCursorCodec::new(uris::EPOCHS_URI),
                position,
            )
            .expect("typed Time cursor"),
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(TimeCursorCodec::new(uris::EPOCHS_URI), wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
impl TryFrom<String> for EpochCursor {
    type Error = TimeResourceError;
    fn try_from(wire: String) -> Result<Self, Self::Error> {
        Self::parse(wire)
    }
}
impl From<EpochCursor> for String {
    fn from(value: EpochCursor) -> Self {
        value.cursor.into_wire()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct EventCursor {
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<EventPosition>>,
}
impl EventCursor {
    fn from_position(position: EventPosition) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(
                TimeCursorCodec::new(uris::EVENTS_URI),
                position,
            )
            .expect("typed Time cursor"),
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(TimeCursorCodec::new(uris::EVENTS_URI), wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
impl TryFrom<String> for EventCursor {
    type Error = TimeResourceError;
    fn try_from(wire: String) -> Result<Self, Self::Error> {
        Self::parse(wire)
    }
}
impl From<EventCursor> for String {
    fn from(value: EventCursor) -> Self {
        value.cursor.into_wire()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct AuthorityCursor {
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<AuthorityReleaseId>>,
}
impl AuthorityCursor {
    fn from_position(position: AuthorityReleaseId) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(
                TimeCursorCodec::new(uris::AUTHORITY_RELEASES_URI),
                position,
            )
            .expect("typed Time cursor"),
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(TimeCursorCodec::new(uris::AUTHORITY_RELEASES_URI), wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
impl TryFrom<String> for AuthorityCursor {
    type Error = TimeResourceError;
    fn try_from(wire: String) -> Result<Self, Self::Error> {
        Self::parse(wire)
    }
}
impl From<AuthorityCursor> for String {
    fn from(value: AuthorityCursor) -> Self {
        value.cursor.into_wire()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct BootstrapAuthorityCursor {
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<AuthorityReleaseId>>,
}
impl BootstrapAuthorityCursor {
    fn from_position(position: AuthorityReleaseId) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(
                TimeCursorCodec::new(uris::BOOTSTRAP_AUTHORITIES_URI),
                position,
            )
            .expect("typed Time cursor"),
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(
            TimeCursorCodec::new(uris::BOOTSTRAP_AUTHORITIES_URI),
            wire,
        )
        .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
impl TryFrom<String> for BootstrapAuthorityCursor {
    type Error = TimeResourceError;
    fn try_from(wire: String) -> Result<Self, Self::Error> {
        Self::parse(wire)
    }
}
impl From<BootstrapAuthorityCursor> for String {
    fn from(value: BootstrapAuthorityCursor) -> Self {
        value.cursor.into_wire()
    }
}
impl CalendarCursor {
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::{CalendarCursor, MissionEpochId, TimeVersion};
    /// CalendarCursor::new(&MissionEpochId::parse("epoch-example").unwrap(), TimeVersion::new(1).unwrap());
    /// ```
    pub fn new(key: &CalendarId, version: TimeVersion) -> Self {
        Self::from_position(VersionPosition {
            key: key.clone(),
            version,
        })
    }
    pub fn calendar_id(&self) -> &CalendarId {
        &self.cursor.position().key
    }
    pub fn version(&self) -> TimeVersion {
        self.cursor.position().version
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
        &self.cursor.position().key
    }
    pub fn version(&self) -> TimeVersion {
        self.cursor.position().version
    }
}

impl EventCursor {
    /// ```compile_fail
    /// use veoveo_time_mcp::{EventCursor, TemporalEventId};
    /// EventCursor::new(&TemporalEventId::parse("event-example").unwrap(), 0, 1_000_000_000);
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
        &self.cursor.position().event_key
    }
    pub fn tai_seconds(&self) -> i64 {
        self.cursor.position().tai_seconds
    }
    pub fn nanosecond(&self) -> SubsecondNanoseconds {
        self.cursor.position().nanosecond
    }
}

impl AuthorityCursor {
    pub fn new(id: &AuthorityReleaseId) -> Self {
        Self::from_position(id.clone())
    }
    pub fn release_id(&self) -> &AuthorityReleaseId {
        self.cursor.position()
    }
}
impl BootstrapAuthorityCursor {
    pub fn new(id: &AuthorityReleaseId) -> Self {
        Self::from_position(id.clone())
    }
    pub fn release_id(&self) -> &AuthorityReleaseId {
        self.cursor.position()
    }
}
