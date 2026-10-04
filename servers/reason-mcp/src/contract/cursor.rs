//! Collection-bound continuation with the published version 1 JSON/base64 profile.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{AnalysisId, ReasonContractError};
use crate::uris;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AnalysisCursor {
    wire: String,
    position: Position,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    created_at: DateTime<Utc>,
    task_id: AnalysisId,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    version: u8,
    collection: String,
    position: Position,
}
impl AnalysisCursor {
    pub fn new(created_at: DateTime<Utc>, analysis_id: AnalysisId) -> Self {
        let position = Position {
            created_at,
            task_id: analysis_id,
        };
        let wire = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&Wire {
                version: 1,
                collection: uris::ANALYSES_URI.into(),
                position: position.clone(),
            })
            .expect("fixed cursor fields serialize"),
        );
        Self { wire, position }
    }
    pub fn parse(value: impl Into<String>) -> Result<Self, ReasonContractError> {
        let wire = value.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(ReasonContractError::InvalidCursor);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(&wire)
            .map_err(|_| ReasonContractError::InvalidCursor)?;
        let value: Wire =
            serde_json::from_slice(&bytes).map_err(|_| ReasonContractError::InvalidCursor)?;
        if value.version != 1 || value.collection != uris::ANALYSES_URI {
            return Err(ReasonContractError::InvalidCursor);
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
    pub fn analysis_id(&self) -> AnalysisId {
        self.position.task_id
    }
}
impl TryFrom<String> for AnalysisCursor {
    type Error = ReasonContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<AnalysisCursor> for String {
    fn from(value: AnalysisCursor) -> Self {
        value.wire
    }
}
impl schemars::JsonSchema for AnalysisCursor {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AnalysisCursor".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}
