//! Domain-owned resource routes, independent of hosted MCP and persistence.
use std::{borrow::Cow, error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceFieldCodec, ResourceRouteError, ResourceTailCodec, ResourceUri,
    ResourceUriError,
};

use super::{AuthorityReleaseId, CalendarId, MissionEpochId, TemporalEventId, TimeVersion};

mod cursor;
mod identifiers;
mod release;
pub use cursor::{
    AuthorityCursor, BootstrapAuthorityCursor, CalendarCursor, EpochCursor, EventCursor,
};
pub use identifiers::{TimeDocument, TimeZoneId};
pub use release::TimeAuthorityReleaseUri;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeResourceError {
    Uri(ResourceUriError),
    UnknownResource,
    InvalidId,
    InvalidVersion,
    InvalidZone,
    InvalidCursor,
}

impl fmt::Display for TimeResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uri(error) => error.fmt(f),
            Self::UnknownResource => f.write_str("unknown or noncanonical Time resource address"),
            Self::InvalidId => f.write_str("invalid Time resource identifier"),
            Self::InvalidVersion => f.write_str("Time version must be in 1..=9223372036854775807"),
            Self::InvalidZone => {
                f.write_str("expected a relative TZDB zone key with nonempty name components")
            }
            Self::InvalidCursor => {
                f.write_str("invalid Time collection cursor or query parameters")
            }
        }
    }
}

impl Error for TimeResourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Uri(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ResourceUriError> for TimeResourceError {
    fn from(error: ResourceUriError) -> Self {
        Self::Uri(error)
    }
}

/// A usable resource carries only the IDs and cursor of its own family.
/// ```compile_fail
/// use veoveo_time_mcp::contract::{CalendarCursor, TimeResource};
/// fn wrong_page(cursor: CalendarCursor) { let _ = TimeResource::Events { cursor: Some(cursor) }; }
/// ```
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(error = TimeResourceError, route_error = time_route_error, wire)]
pub enum TimeResource {
    #[resource(template = "time://docs")]
    Docs,
    #[resource(template = "time://docs/{doc_id}")]
    Document(
        #[resource(variable = "doc_id", codec = TimeDocumentCodec, error = |error| error)]
        TimeDocument,
    ),
    #[resource(template = "time://contract")]
    Contract,
    #[resource(template = "ui://time/timeline.html")]
    TimelineApp,
    #[resource(template = "time://clock/current")]
    ClockCurrent,
    #[resource(template = "time://clock/quality")]
    ClockQuality,
    #[resource(template = "time://authorities/current")]
    AuthoritiesCurrent,
    #[resource(template = "time://authorities/releases/{release_id}")]
    AuthorityRelease(
        #[resource(variable = "release_id", error = |_| TimeResourceError::InvalidId)]
        AuthorityReleaseId,
    ),
    #[resource(template = "time://authorities/releases{?cursor}", route_error = page_route_error)]
    AuthorityReleases {
        #[resource(codec = TimeCursorCodec, error = |error| error)]
        cursor: Option<AuthorityCursor>,
    },
    #[resource(template = "time://authorities/bootstrap/{release_id}")]
    BootstrapAuthority(
        #[resource(variable = "release_id", error = |_| TimeResourceError::InvalidId)]
        AuthorityReleaseId,
    ),
    #[resource(template = "time://authorities/bootstrap{?cursor}", route_error = page_route_error)]
    BootstrapAuthorities {
        #[resource(codec = TimeCursorCodec, error = |error| error)]
        cursor: Option<BootstrapAuthorityCursor>,
    },
    #[resource(template = "time://zones/{+zone_id}")]
    Zone(
        #[resource(variable = "zone_id", tail, codec = TimeZoneCodec, error = |error| error)]
        TimeZoneId,
    ),
    #[resource(template = "time://calendars{?cursor}", route_error = page_route_error)]
    Calendars {
        #[resource(codec = TimeCursorCodec, error = |error| error)]
        cursor: Option<CalendarCursor>,
    },
    #[resource(template = "time://calendars/{calendar_id}/versions/{version}")]
    Calendar {
        #[resource(variable = "calendar_id", error = |_| TimeResourceError::InvalidId)]
        id: CalendarId,
        #[resource(codec = TimeVersionCodec, error = |error| error)]
        version: TimeVersion,
    },
    #[resource(template = "time://epochs{?cursor}", route_error = page_route_error)]
    Epochs {
        #[resource(codec = TimeCursorCodec, error = |error| error)]
        cursor: Option<EpochCursor>,
    },
    #[resource(template = "time://epochs/{epoch_id}")]
    Epoch(
        #[resource(variable = "epoch_id", error = |_| TimeResourceError::InvalidId)] MissionEpochId,
    ),
    #[resource(template = "time://epochs/{epoch_id}/versions/{version}")]
    EpochVersion {
        #[resource(variable = "epoch_id", error = |_| TimeResourceError::InvalidId)]
        id: MissionEpochId,
        #[resource(codec = TimeVersionCodec, error = |error| error)]
        version: TimeVersion,
    },
    #[resource(template = "time://events{?cursor}", route_error = page_route_error)]
    Events {
        #[resource(codec = TimeCursorCodec, error = |error| error)]
        cursor: Option<EventCursor>,
    },
    #[resource(template = "time://events/{event_id}")]
    Event(
        #[resource(variable = "event_id", error = |_| TimeResourceError::InvalidId)]
        TemporalEventId,
    ),
}

impl TimeResource {
    pub fn parse(value: &str) -> Result<Self, TimeResourceError> {
        let uri = ResourceUri::new(value)?;
        <Self as ResourceAddress>::parse(&uri)
    }

    /// Cursor pages are invalidated through their collection root.
    pub fn is_subscribable(&self) -> bool {
        matches!(
            self,
            Self::ClockCurrent
                | Self::ClockQuality
                | Self::AuthoritiesCurrent
                | Self::Calendars { cursor: None }
                | Self::Epochs { cursor: None }
                | Self::Events { cursor: None }
                | Self::Calendar { .. }
                | Self::Epoch(_)
                | Self::Event(_)
        )
    }
}

impl fmt::Display for TimeResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri()
            .expect("validated Time resource components")
            .fmt(f)
    }
}

impl From<super::TimeVersionError> for TimeResourceError {
    fn from(_: super::TimeVersionError) -> Self {
        Self::InvalidVersion
    }
}

fn time_route_error(error: ResourceRouteError) -> TimeResourceError {
    match error {
        ResourceRouteError::Uri(error) => TimeResourceError::Uri(error),
        _ => TimeResourceError::UnknownResource,
    }
}

fn page_route_error(error: ResourceRouteError) -> TimeResourceError {
    match error {
        ResourceRouteError::Query => TimeResourceError::InvalidCursor,
        _ => time_route_error(error),
    }
}

struct TimeDocumentCodec;
impl ResourceFieldCodec<TimeDocument> for TimeDocumentCodec {
    type Error = TimeResourceError;
    fn parse(value: &str) -> Result<TimeDocument, Self::Error> {
        TimeDocument::parse(value)
    }
    fn text(value: &TimeDocument) -> Cow<'_, str> {
        value.as_str().into()
    }
}

struct TimeVersionCodec;
impl ResourceFieldCodec<TimeVersion> for TimeVersionCodec {
    type Error = TimeResourceError;
    fn parse(value: &str) -> Result<TimeVersion, Self::Error> {
        TimeVersion::new(
            value
                .parse()
                .map_err(|_| TimeResourceError::InvalidVersion)?,
        )
        .map_err(Into::into)
    }
    fn text(value: &TimeVersion) -> Cow<'_, str> {
        value.get().to_string().into()
    }
}

struct TimeZoneCodec;
impl ResourceTailCodec<TimeZoneId> for TimeZoneCodec {
    type Error = TimeResourceError;
    fn parse(segments: &[Cow<'_, str>]) -> Result<TimeZoneId, Self::Error> {
        TimeZoneId::new(
            segments
                .iter()
                .map(|part| part.as_ref())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }
    fn segments(value: &TimeZoneId) -> Vec<Cow<'_, str>> {
        value.components().map(Cow::Borrowed).collect()
    }
}

struct TimeCursorCodec;
impl ResourceFieldCodec<AuthorityCursor> for TimeCursorCodec {
    type Error = TimeResourceError;
    fn parse(value: &str) -> Result<AuthorityCursor, Self::Error> {
        AuthorityCursor::parse(value)
    }
    fn text(value: &AuthorityCursor) -> Cow<'_, str> {
        value.as_str().into()
    }
}
impl ResourceFieldCodec<BootstrapAuthorityCursor> for TimeCursorCodec {
    type Error = TimeResourceError;
    fn parse(value: &str) -> Result<BootstrapAuthorityCursor, Self::Error> {
        BootstrapAuthorityCursor::parse(value)
    }
    fn text(value: &BootstrapAuthorityCursor) -> Cow<'_, str> {
        value.as_str().into()
    }
}
impl ResourceFieldCodec<CalendarCursor> for TimeCursorCodec {
    type Error = TimeResourceError;
    fn parse(value: &str) -> Result<CalendarCursor, Self::Error> {
        CalendarCursor::parse(value)
    }
    fn text(value: &CalendarCursor) -> Cow<'_, str> {
        value.as_str().into()
    }
}
impl ResourceFieldCodec<EpochCursor> for TimeCursorCodec {
    type Error = TimeResourceError;
    fn parse(value: &str) -> Result<EpochCursor, Self::Error> {
        EpochCursor::parse(value)
    }
    fn text(value: &EpochCursor) -> Cow<'_, str> {
        value.as_str().into()
    }
}
impl ResourceFieldCodec<EventCursor> for TimeCursorCodec {
    type Error = TimeResourceError;
    fn parse(value: &str) -> Result<EventCursor, Self::Error> {
        EventCursor::parse(value)
    }
    fn text(value: &EventCursor) -> Cow<'_, str> {
        value.as_str().into()
    }
}
