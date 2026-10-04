//! Collection-bound continuation with the published version 1 JSON/base64 profile.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{RunId, SessionId, StreamContractError};
use crate::uris;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunCursor {
    cursor: veoveo_types::OpaqueCursor<RunCursorCodec>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    created_at: DateTime<Utc>,
    task_id: RunId,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    version: u8,
    collection: String,
    position: Position,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RunCursorCodec;
impl veoveo_types::StatelessCursorCodec for RunCursorCodec {}
impl veoveo_types::CursorCodec for RunCursorCodec {
    type Position = Position;
    type Error = StreamContractError;
    fn check(&self, _position: &Position) -> Result<(), StreamContractError> {
        Ok(())
    }
    fn encode(&self, position: &Position) -> Result<String, StreamContractError> {
        let bytes = serde_json::to_vec(&Wire {
            version: 1,
            collection: uris::RUNS_URI.to_owned(),
            position: position.clone(),
        })
        .expect("closed cursor fields serialize");
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Position, StreamContractError> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(StreamContractError::InvalidCursor);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| StreamContractError::InvalidCursor)?;
        let value: Wire =
            serde_json::from_slice(&bytes).map_err(|_| StreamContractError::InvalidCursor)?;
        if value.version != 1 || value.collection != uris::RUNS_URI {
            return Err(StreamContractError::InvalidCursor);
        }
        Ok(value.position)
    }
}
impl RunCursor {
    pub fn new(created_at: DateTime<Utc>, run_id: RunId) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(
                RunCursorCodec,
                Position {
                    created_at,
                    task_id: run_id,
                },
            )
            .expect("typed cursor position"),
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, StreamContractError> {
        veoveo_types::OpaqueCursor::parse(RunCursorCodec, wire).map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.cursor.position().created_at
    }
    pub fn run_id(&self) -> RunId {
        self.cursor.position().task_id
    }
}
impl schemars::JsonSchema for RunCursor {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RunCursor".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}

/// Continuation in the process-local session collection, newest IDs first.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionCursor {
    cursor: veoveo_types::OpaqueCursor<SessionCursorCodec>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionWire {
    version: u8,
    collection: String,
    position: SessionId,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SessionCursorCodec;
impl veoveo_types::StatelessCursorCodec for SessionCursorCodec {}
impl veoveo_types::CursorCodec for SessionCursorCodec {
    type Position = SessionId;
    type Error = StreamContractError;
    fn check(&self, _position: &SessionId) -> Result<(), StreamContractError> {
        Ok(())
    }
    fn encode(&self, position: &SessionId) -> Result<String, StreamContractError> {
        let bytes = serde_json::to_vec(&SessionWire {
            version: 1,
            collection: uris::SESSIONS_URI.to_owned(),
            position: *position,
        })
        .expect("closed cursor fields serialize");
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<SessionId, StreamContractError> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(StreamContractError::InvalidCursor);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| StreamContractError::InvalidCursor)?;
        let value: SessionWire =
            serde_json::from_slice(&bytes).map_err(|_| StreamContractError::InvalidCursor)?;
        if value.version != 1 || value.collection != uris::SESSIONS_URI {
            return Err(StreamContractError::InvalidCursor);
        }
        Ok(value.position)
    }
}
impl SessionCursor {
    pub fn new(position: SessionId) -> Self {
        Self {
            cursor: veoveo_types::OpaqueCursor::try_new(SessionCursorCodec, position)
                .expect("typed cursor position"),
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, StreamContractError> {
        veoveo_types::OpaqueCursor::parse(SessionCursorCodec, wire).map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
    pub fn session_id(&self) -> SessionId {
        *self.cursor.position()
    }
}
impl schemars::JsonSchema for SessionCursor {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SessionCursor".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}
