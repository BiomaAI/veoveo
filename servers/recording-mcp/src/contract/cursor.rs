//! A catalog-owned position, converted to Store's cursor only at a query boundary.
use super::{RecordingContractError, RecordingId, ids::string_schema};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const RECORDING_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordingCatalogCursor {
    wire: String,
    position: Position,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    started_at: DateTime<Utc>,
    recording_id: RecordingId,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    collection: String,
    position: Position,
}

impl RecordingCatalogCursor {
    pub fn new(started_at: DateTime<Utc>, recording_id: RecordingId) -> Self {
        let position = Position {
            started_at,
            recording_id,
        };
        let wire = hex::encode(
            serde_json::to_vec(&Envelope {
                version: 1,
                collection: crate::uris::CATALOG_URI.into(),
                position: position.clone(),
            })
            .expect("catalog position serialization"),
        );
        Self { wire, position }
    }
    pub fn parse(value: impl Into<String>) -> Result<Self, RecordingContractError> {
        let value = value.into();
        let invalid = || RecordingContractError::Cursor;
        if value.is_empty() || value.len() > 2048 {
            return Err(invalid());
        }
        let bytes = hex::decode(&value).map_err(|_| invalid())?;
        let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if envelope.version != 1 || envelope.collection != crate::uris::CATALOG_URI {
            return Err(invalid());
        }
        let result = Self::new(envelope.position.started_at, envelope.position.recording_id);
        if result.wire != value {
            return Err(invalid());
        }
        Ok(result)
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
    pub fn started_at(&self) -> DateTime<Utc> {
        self.position.started_at
    }
    pub fn recording_id(&self) -> RecordingId {
        self.position.recording_id
    }
}
impl TryFrom<String> for RecordingCatalogCursor {
    type Error = RecordingContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<RecordingCatalogCursor> for String {
    fn from(value: RecordingCatalogCursor) -> Self {
        value.wire
    }
}
impl std::fmt::Display for RecordingCatalogCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.wire.fmt(f)
    }
}
string_schema!(RecordingCatalogCursor);
