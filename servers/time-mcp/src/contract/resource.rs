//! Domain-owned resource routes, independent of hosted MCP and persistence.
use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriError, ResourceUriParts,
    UriSegment,
};

use super::{AuthorityReleaseId, CalendarId, MissionEpochId, TemporalEventId, TimeVersion};
use crate::uris;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub enum TimeResource {
    Docs,
    Document(TimeDocument),
    Contract,
    TimelineApp,
    ClockCurrent,
    ClockQuality,
    AuthoritiesCurrent,
    AuthorityRelease(AuthorityReleaseId),
    AuthorityReleases {
        cursor: Option<AuthorityCursor>,
    },
    BootstrapAuthority(AuthorityReleaseId),
    BootstrapAuthorities {
        cursor: Option<BootstrapAuthorityCursor>,
    },
    Zone(TimeZoneId),
    Calendars {
        cursor: Option<CalendarCursor>,
    },
    Calendar {
        id: CalendarId,
        version: TimeVersion,
    },
    Epochs {
        cursor: Option<EpochCursor>,
    },
    Epoch(MissionEpochId),
    EpochVersion {
        id: MissionEpochId,
        version: TimeVersion,
    },
    Events {
        cursor: Option<EventCursor>,
    },
    Event(TemporalEventId),
}

impl TimeResource {
    pub fn parse(value: &str) -> Result<Self, TimeResourceError> {
        let parts = ResourceUriParts::parse(value)?;
        let decoded: Vec<_> = parts.path_segments().collect();
        let path: Vec<_> = decoded.iter().map(|part| part.as_ref()).collect();
        let page = match (parts.scheme(), parts.authority(), path.as_slice()) {
            ("time", "calendars", []) => Some(Self::Calendars {
                cursor: page_cursor(&parts, CalendarCursor::parse)?,
            }),
            ("time", "epochs", []) => Some(Self::Epochs {
                cursor: page_cursor(&parts, EpochCursor::parse)?,
            }),
            ("time", "authorities", ["releases"]) => Some(Self::AuthorityReleases {
                cursor: page_cursor(&parts, AuthorityCursor::parse)?,
            }),
            ("time", "authorities", ["bootstrap"]) => Some(Self::BootstrapAuthorities {
                cursor: page_cursor(&parts, BootstrapAuthorityCursor::parse)?,
            }),
            ("time", "events", []) => Some(Self::Events {
                cursor: page_cursor(&parts, EventCursor::parse)?,
            }),
            _ => None,
        };
        let resource = if let Some(page) = page {
            page
        } else {
            if parts.has_query() {
                return Err(TimeResourceError::UnknownResource);
            }
            match (parts.scheme(), parts.authority(), path.as_slice()) {
                ("time", "docs", []) => Self::Docs,
                ("time", "docs", [doc]) => Self::Document(TimeDocument::parse(doc)?),
                ("time", "contract", []) => Self::Contract,
                ("ui", "time", ["timeline.html"]) => Self::TimelineApp,
                ("time", "clock", ["current"]) => Self::ClockCurrent,
                ("time", "clock", ["quality"]) => Self::ClockQuality,
                ("time", "authorities", ["current"]) => Self::AuthoritiesCurrent,
                ("time", "authorities", ["releases", id]) => Self::AuthorityRelease(
                    AuthorityReleaseId::new(*id).map_err(|_| TimeResourceError::InvalidId)?,
                ),
                ("time", "authorities", ["bootstrap", id]) => Self::BootstrapAuthority(
                    AuthorityReleaseId::new(*id).map_err(|_| TimeResourceError::InvalidId)?,
                ),
                ("time", "epochs", [id, "versions", version]) => Self::EpochVersion {
                    id: MissionEpochId::new(*id).map_err(|_| TimeResourceError::InvalidId)?,
                    version: TimeVersion::new(
                        version
                            .parse()
                            .map_err(|_| TimeResourceError::InvalidVersion)?,
                    )?,
                },
                ("time", "zones", components) => {
                    // A TZDB key is itself a slash-separated domain name. URI
                    // splitting/decoding has already happened in the shared parser.
                    Self::Zone(TimeZoneId::new(components.join("/"))?)
                }
                ("time", "calendars", [id, "versions", version]) => Self::Calendar {
                    id: CalendarId::new(*id).map_err(|_| TimeResourceError::InvalidId)?,
                    version: TimeVersion::new(
                        version
                            .parse()
                            .map_err(|_| TimeResourceError::InvalidVersion)?,
                    )?,
                },
                ("time", "epochs", [id]) => {
                    Self::Epoch(MissionEpochId::new(*id).map_err(|_| TimeResourceError::InvalidId)?)
                }
                ("time", "events", [id]) => Self::Event(
                    TemporalEventId::new(*id).map_err(|_| TimeResourceError::InvalidId)?,
                ),
                _ => return Err(TimeResourceError::UnknownResource),
            }
        };
        if resource.to_uri()?.as_str() != value {
            return Err(TimeResourceError::UnknownResource);
        }
        Ok(resource)
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

fn page_cursor<C>(
    parts: &ResourceUriParts,
    parse: impl FnOnce(String) -> Result<C, TimeResourceError>,
) -> Result<Option<C>, TimeResourceError> {
    if !parts.has_query() {
        return Ok(None);
    }
    let query = parts.query_parameters();
    if query.len() != 1 {
        return Err(TimeResourceError::InvalidCursor);
    }
    let cursor = query
        .get("cursor")
        .ok_or(TimeResourceError::InvalidCursor)?;
    parse(cursor.clone()).map(Some)
}

impl ResourceAddress for TimeResource {
    type Error = TimeResourceError;

    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }

    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let (base, segments, cursor): (&str, Vec<String>, Option<&str>) = match self {
            Self::Docs => (uris::DOCS_URI, vec![], None),
            Self::Document(doc) => (uris::DOCS_URI, vec![doc.as_str().into()], None),
            Self::Contract => (uris::CONTRACT_URI, vec![], None),
            Self::TimelineApp => (uris::TIMELINE_APP_URI, vec![], None),
            Self::ClockCurrent => (uris::CLOCK_CURRENT_URI, vec![], None),
            Self::ClockQuality => (uris::CLOCK_QUALITY_URI, vec![], None),
            Self::AuthoritiesCurrent => (uris::AUTHORITIES_CURRENT_URI, vec![], None),
            Self::AuthorityRelease(id) => {
                ("time://authorities/releases", vec![id.to_string()], None)
            }
            Self::AuthorityReleases { cursor } => (
                uris::AUTHORITY_RELEASES_URI,
                vec![],
                cursor.as_ref().map(AuthorityCursor::as_str),
            ),
            Self::BootstrapAuthorities { cursor } => (
                uris::BOOTSTRAP_AUTHORITIES_URI,
                vec![],
                cursor.as_ref().map(BootstrapAuthorityCursor::as_str),
            ),
            Self::BootstrapAuthority(id) => {
                (uris::BOOTSTRAP_AUTHORITIES_URI, vec![id.to_string()], None)
            }
            Self::EpochVersion { id, version } => (
                uris::EPOCHS_URI,
                vec![id.to_string(), "versions".into(), version.get().to_string()],
                None,
            ),
            Self::Zone(id) => (
                "time://zones",
                id.components().map(str::to_owned).collect(),
                None,
            ),
            Self::Calendars { cursor } => (
                uris::CALENDARS_URI,
                vec![],
                cursor.as_ref().map(CalendarCursor::as_str),
            ),
            Self::Calendar { id, version } => (
                uris::CALENDARS_URI,
                vec![id.to_string(), "versions".into(), version.get().to_string()],
                None,
            ),
            Self::Epochs { cursor } => (
                uris::EPOCHS_URI,
                vec![],
                cursor.as_ref().map(EpochCursor::as_str),
            ),
            Self::Epoch(id) => (uris::EPOCHS_URI, vec![id.to_string()], None),
            Self::Events { cursor } => (
                uris::EVENTS_URI,
                vec![],
                cursor.as_ref().map(EventCursor::as_str),
            ),
            Self::Event(id) => (uris::EVENTS_URI, vec![id.to_string()], None),
        };
        let mut builder = ResourceUriBuilder::new(base)?;
        for segment in segments {
            builder = builder.segment(UriSegment::new(segment)?);
        }
        if let Some(cursor) = cursor {
            builder = builder.query_pair("cursor", cursor)?;
        }
        Ok(builder.build()?)
    }
}

impl TryFrom<String> for TimeResource {
    type Error = TimeResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<TimeResource> for String {
    fn from(value: TimeResource) -> Self {
        value
            .to_uri()
            .expect("validated Time resource components")
            .into()
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
