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

trait CursorCollection {
    type Position: Serialize + DeserializeOwned;
    const URI: &'static str;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TimeCursorCodec<C>(std::marker::PhantomData<C>);
impl<C: CursorCollection + Default> veoveo_types::StatelessCursorCodec for TimeCursorCodec<C> {}
impl<C: CursorCollection> veoveo_types::CursorCodec for TimeCursorCodec<C> {
    type Position = C::Position;
    type Error = TimeResourceError;
    fn check(&self, _position: &Self::Position) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        Ok(hex::encode(
            serde_json::to_vec(&WireCursor {
                version: 1,
                collection: C::URI.to_owned(),
                position,
            })
            .expect("closed Time cursor fields serialize"),
        ))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        decode(wire, C::URI)
    }
}

fn admit_position<C: CursorCollection + Default>(
    position: C::Position,
) -> veoveo_types::OpaqueCursor<TimeCursorCodec<C>> {
    veoveo_types::OpaqueCursor::try_new(TimeCursorCodec::<C>::default(), position)
        .expect("typed Time cursor")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CalendarCollection;
impl CursorCollection for CalendarCollection {
    type Position = VersionPosition<CalendarId>;
    const URI: &'static str = uris::CALENDARS_URI;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct CalendarCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<CalendarCollection>>,
}
impl CalendarCursor {
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(TimeCursorCodec::<CalendarCollection>::default(), wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EpochCollection;
impl CursorCollection for EpochCollection {
    type Position = VersionPosition<MissionEpochId>;
    const URI: &'static str = uris::EPOCHS_URI;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EpochCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<EpochCollection>>,
}
impl EpochCursor {
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(TimeCursorCodec::<EpochCollection>::default(), wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EventCollection;
impl CursorCollection for EventCollection {
    type Position = EventPosition;
    const URI: &'static str = uris::EVENTS_URI;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EventCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<EventCollection>>,
}
impl EventCursor {
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(TimeCursorCodec::<EventCollection>::default(), wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct AuthorityCollection;
impl CursorCollection for AuthorityCollection {
    type Position = AuthorityReleaseId;
    const URI: &'static str = uris::AUTHORITY_RELEASES_URI;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct AuthorityCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<AuthorityCollection>>,
}
impl AuthorityCursor {
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(TimeCursorCodec::<AuthorityCollection>::default(), wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BootstrapAuthorityCollection;
impl CursorCollection for BootstrapAuthorityCollection {
    type Position = AuthorityReleaseId;
    const URI: &'static str = uris::BOOTSTRAP_AUTHORITIES_URI;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct BootstrapAuthorityCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<TimeCursorCodec<BootstrapAuthorityCollection>>,
}
impl BootstrapAuthorityCursor {
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeResourceError> {
        veoveo_types::OpaqueCursor::parse(
            TimeCursorCodec::<BootstrapAuthorityCollection>::default(),
            wire,
        )
        .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}

impl CalendarCursor {
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::{CalendarCursor, MissionEpochId, TimeVersion};
    /// CalendarCursor::new(&MissionEpochId::parse("epoch-example").unwrap(), TimeVersion::new(1).unwrap());
    /// ```
    pub fn new(key: &CalendarId, version: TimeVersion) -> Self {
        Self {
            cursor: admit_position::<CalendarCollection>(VersionPosition {
                key: key.clone(),
                version,
            }),
        }
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
        Self {
            cursor: admit_position::<EpochCollection>(VersionPosition {
                key: key.clone(),
                version,
            }),
        }
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
        Self {
            cursor: admit_position::<EventCollection>(position),
        }
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
        Self {
            cursor: admit_position::<AuthorityCollection>(id.clone()),
        }
    }
    pub fn release_id(&self) -> &AuthorityReleaseId {
        self.cursor.position()
    }
}
impl BootstrapAuthorityCursor {
    pub fn new(id: &AuthorityReleaseId) -> Self {
        Self {
            cursor: admit_position::<BootstrapAuthorityCollection>(id.clone()),
        }
    }
    pub fn release_id(&self) -> &AuthorityReleaseId {
        self.cursor.position()
    }
}
