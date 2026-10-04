//! Producer state carries the Recording owner's admitted identity.
use super::{RecordingKey, RecordingPublisherLifecycle};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_recording_contract::{RecordingId, RecordingUri};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordingCatalogLifecycle {
    Pending,
    Ready,
    Unavailable,
    Invalid,
}

/// Catalog availability and identity change together. Readiness requires a typed URI.
/// ```compile_fail
/// use veoveo_uav_sim_mcp::contract::RecordingCatalog;
/// let catalog = RecordingCatalog::Ready("recording://recordings/arbitrary".to_owned());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum RecordingCatalog {
    Pending { diagnostic: Option<String> },
    Ready(RecordingUri),
    Unavailable { diagnostic: Option<String> },
    Invalid { diagnostic: Option<String> },
}

impl RecordingCatalog {
    pub fn lifecycle(&self) -> RecordingCatalogLifecycle {
        match self {
            Self::Pending { .. } => RecordingCatalogLifecycle::Pending,
            Self::Ready(_) => RecordingCatalogLifecycle::Ready,
            Self::Unavailable { .. } => RecordingCatalogLifecycle::Unavailable,
            Self::Invalid { .. } => RecordingCatalogLifecycle::Invalid,
        }
    }

    pub fn recording_uri(&self) -> Option<&RecordingUri> {
        match self {
            Self::Ready(uri) => Some(uri),
            _ => None,
        }
    }

    pub fn recording_id(&self) -> Option<RecordingId> {
        self.recording_uri().map(RecordingUri::id)
    }

    pub fn diagnostic(&self) -> Option<&str> {
        match self {
            Self::Ready(_) => None,
            Self::Pending { diagnostic }
            | Self::Unavailable { diagnostic }
            | Self::Invalid { diagnostic } => diagnostic.as_deref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RecordingStateWire", into = "RecordingStateWire")]
pub struct RecordingState {
    pub recording_key: RecordingKey,
    pub catalog: RecordingCatalog,
    pub active: bool,
    pub publisher_lifecycle: RecordingPublisherLifecycle,
    pub queue_capacity: u32,
    pub queued_events: u32,
    pub dropped_events: u64,
    pub publisher_diagnostic: Option<String>,
    pub camera_streams: Vec<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecordingStateWire {
    recording_key: RecordingKey,
    catalog_lifecycle: RecordingCatalogLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recording_id: Option<RecordingId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recording_uri: Option<RecordingUri>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    catalog_diagnostic: Option<String>,
    active: bool,
    publisher_lifecycle: RecordingPublisherLifecycle,
    queue_capacity: u32,
    queued_events: u32,
    dropped_events: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publisher_diagnostic: Option<String>,
    camera_streams: Vec<String>,
    started_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("UAV recording catalog state and identity disagree")]
pub struct RecordingCatalogError;

impl TryFrom<RecordingStateWire> for RecordingState {
    type Error = RecordingCatalogError;
    fn try_from(value: RecordingStateWire) -> Result<Self, Self::Error> {
        let catalog = match (
            value.catalog_lifecycle,
            value.recording_id,
            value.recording_uri,
        ) {
            (RecordingCatalogLifecycle::Ready, Some(id), Some(uri))
                if uri.id() == id && value.catalog_diagnostic.is_none() =>
            {
                RecordingCatalog::Ready(uri)
            }
            (RecordingCatalogLifecycle::Pending, None, None) => RecordingCatalog::Pending {
                diagnostic: value.catalog_diagnostic,
            },
            (RecordingCatalogLifecycle::Unavailable, None, None) => RecordingCatalog::Unavailable {
                diagnostic: value.catalog_diagnostic,
            },
            (RecordingCatalogLifecycle::Invalid, None, None) => RecordingCatalog::Invalid {
                diagnostic: value.catalog_diagnostic,
            },
            _ => return Err(RecordingCatalogError),
        };
        Ok(Self {
            recording_key: value.recording_key,
            catalog,
            active: value.active,
            publisher_lifecycle: value.publisher_lifecycle,
            queue_capacity: value.queue_capacity,
            queued_events: value.queued_events,
            dropped_events: value.dropped_events,
            publisher_diagnostic: value.publisher_diagnostic,
            camera_streams: value.camera_streams,
            started_at: value.started_at,
        })
    }
}

impl From<RecordingState> for RecordingStateWire {
    fn from(value: RecordingState) -> Self {
        Self {
            recording_key: value.recording_key,
            catalog_lifecycle: value.catalog.lifecycle(),
            recording_id: value.catalog.recording_id(),
            recording_uri: value.catalog.recording_uri().cloned(),
            catalog_diagnostic: value.catalog.diagnostic().map(str::to_owned),
            active: value.active,
            publisher_lifecycle: value.publisher_lifecycle,
            queue_capacity: value.queue_capacity,
            queued_events: value.queued_events,
            dropped_events: value.dropped_events,
            publisher_diagnostic: value.publisher_diagnostic,
            camera_streams: value.camera_streams,
            started_at: value.started_at,
        }
    }
}
