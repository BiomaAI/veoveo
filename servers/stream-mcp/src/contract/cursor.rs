//! Collection-bound continuation with the published version 1 JSON/base64 profile.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{RunId, SessionId, StreamContractError};
use crate::uris;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RunCursor {
    wire: String,
    position: Position,
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
impl RunCursor {
    pub fn new(created_at: DateTime<Utc>, run_id: RunId) -> Self {
        let position = Position {
            created_at,
            task_id: run_id,
        };
        let wire = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&Wire {
                version: 1,
                collection: uris::RUNS_URI.into(),
                position: position.clone(),
            })
            .expect("fixed cursor fields serialize"),
        );
        Self { wire, position }
    }
    pub fn parse(value: impl Into<String>) -> Result<Self, StreamContractError> {
        let wire = value.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(StreamContractError::InvalidCursor);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(&wire)
            .map_err(|_| StreamContractError::InvalidCursor)?;
        let value: Wire =
            serde_json::from_slice(&bytes).map_err(|_| StreamContractError::InvalidCursor)?;
        if value.version != 1 || value.collection != uris::RUNS_URI {
            return Err(StreamContractError::InvalidCursor);
        }
        Ok(Self {
            wire,
            position: value.position,
        })
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.position.created_at
    }
    pub fn run_id(&self) -> RunId {
        self.position.task_id
    }
}
impl TryFrom<String> for RunCursor {
    type Error = StreamContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<RunCursor> for String {
    fn from(value: RunCursor) -> Self {
        value.wire
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
#[serde(try_from = "String", into = "String")]
pub struct SessionCursor {
    wire: String,
    position: SessionId,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionWire {
    version: u8,
    collection: String,
    position: SessionId,
}
impl SessionCursor {
    pub fn new(position: SessionId) -> Self {
        let wire = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&SessionWire {
                version: 1,
                collection: uris::SESSIONS_URI.into(),
                position,
            })
            .expect("fixed cursor fields serialize"),
        );
        Self { wire, position }
    }
    pub fn parse(value: impl Into<String>) -> Result<Self, StreamContractError> {
        let wire = value.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(StreamContractError::InvalidCursor);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(&wire)
            .map_err(|_| StreamContractError::InvalidCursor)?;
        let value: SessionWire =
            serde_json::from_slice(&bytes).map_err(|_| StreamContractError::InvalidCursor)?;
        if value.version != 1 || value.collection != uris::SESSIONS_URI {
            return Err(StreamContractError::InvalidCursor);
        }
        Ok(Self {
            wire,
            position: value.position,
        })
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
    pub fn session_id(&self) -> SessionId {
        self.position
    }
}
impl TryFrom<String> for SessionCursor {
    type Error = StreamContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<SessionCursor> for String {
    fn from(value: SessionCursor) -> Self {
        value.wire
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
