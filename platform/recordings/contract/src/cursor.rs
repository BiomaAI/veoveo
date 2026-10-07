//! A catalog-owned position, converted to Store's cursor only at a query boundary.
use super::{RecordingContractError, RecordingId, ids::string_schema};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const RECORDING_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordingCatalogCursor {
    cursor: veoveo_types::OpaqueCursor<RecordingCatalogCursorCodec>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Position {
    started_at: DateTime<Utc>,
    recording_id: RecordingId,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Envelope {
    version: u8,
    collection: String,
    position: Position,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RecordingCatalogCursorCodec;
impl veoveo_types::StatelessCursorCodec for RecordingCatalogCursorCodec {}
impl veoveo_types::CursorCodec for RecordingCatalogCursorCodec {
    type Position = Position;
    type Error = RecordingContractError;
    fn check(&self, _position: &Position) -> Result<(), RecordingContractError> {
        Ok(())
    }
    fn encode(&self, position: &Position) -> Result<String, RecordingContractError> {
        let bytes = serde_json::to_vec(&Envelope {
            version: 2,
            collection: crate::uris::CATALOG_URI.to_owned(),
            position: position.clone(),
        })
        .expect("closed cursor fields serialize");
        Ok(hex::encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Position, RecordingContractError> {
        if wire.is_empty() || wire.len() > 2048 {
            return Err(RecordingContractError::Cursor);
        }
        let bytes = hex::decode(wire).map_err(|_| RecordingContractError::Cursor)?;
        let value: Envelope =
            serde_json::from_slice(&bytes).map_err(|_| RecordingContractError::Cursor)?;
        if value.version != 2 || value.collection != crate::uris::CATALOG_URI {
            return Err(RecordingContractError::Cursor);
        }
        if self.encode(&value.position)? != wire {
            return Err(RecordingContractError::Cursor);
        }
        Ok(value.position)
    }
}
impl RecordingCatalogCursor {
    pub fn new(started_at: DateTime<Utc>, recording_id: RecordingId) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(
                RecordingCatalogCursorCodec,
                Position {
                    started_at,
                    recording_id,
                },
            )
            .expect("typed cursor position"),
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, RecordingContractError> {
        veoveo_types::OpaqueCursor::parse(RecordingCatalogCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
    pub fn started_at(&self) -> DateTime<Utc> {
        self.cursor.position().started_at
    }
    pub fn recording_id(&self) -> RecordingId {
        self.cursor.position().recording_id
    }
}
impl std::fmt::Display for RecordingCatalogCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cursor.as_str().fmt(f)
    }
}
impl schemars::JsonSchema for RecordingCatalogCursor {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RecordingCatalogCursor".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        string_schema(generator)
    }
}
