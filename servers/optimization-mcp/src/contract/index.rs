//! Collection positions and addresses; a cursor carries no caller authority.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, TaskId};

pub const OPTIMIZATION_INDEX_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationCollection {
    Problems,
    Runs,
    Solutions,
}

impl OptimizationCollection {
    pub fn root(self) -> &'static str {
        match self {
            Self::Problems => "optimization://problems",
            Self::Runs => "optimization://runs",
            Self::Solutions => "optimization://solutions",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected an Optimization collection address and a matching version 1 native Task cursor")]
pub struct OptimizationIndexError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    collection: OptimizationCollection,
    created_at: DateTime<Utc>,
    task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct OptimizationIndexCursor {
    cursor: veoveo_types::OpaqueCursor<OptimizationIndexCursorCodec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OptimizationIndexCursorCodec;
impl veoveo_types::CursorCodec for OptimizationIndexCursorCodec {
    type Position = CursorWire;
    type Error = OptimizationIndexError;
    fn check(&self, position: &CursorWire) -> Result<(), Self::Error> {
        OptimizationIndexCursor::validate(position.task_id)
    }
    fn encode(&self, position: &CursorWire) -> Result<String, Self::Error> {
        Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(position).expect("closed cursor serializes")))
    }
    fn decode(&self, wire: &str) -> Result<CursorWire, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(OptimizationIndexError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| OptimizationIndexError)?;
        let position: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| OptimizationIndexError)?;
        if position.version != 1 {
            return Err(OptimizationIndexError);
        }
        Ok(position)
    }
}
impl OptimizationIndexCursor {
    /// ```compile_fail
    /// use veoveo_optimization_mcp::contract::{OptimizationCollection, OptimizationIndexCursor};
    /// OptimizationIndexCursor::new(OptimizationCollection::Runs, chrono::Utc::now(), "raw-task");
    /// ```
    pub fn new(
        collection: OptimizationCollection,
        created_at: DateTime<Utc>,
        task_id: TaskId,
    ) -> Result<Self, OptimizationIndexError> {
        veoveo_types::OpaqueCursor::try_new(
            OptimizationIndexCursorCodec,
            CursorWire {
                version: 1,
                collection,
                created_at,
                task_id,
            },
        )
        .map(|cursor| Self { cursor })
    }

    pub fn parse(wire: impl Into<String>) -> Result<Self, OptimizationIndexError> {
        veoveo_types::OpaqueCursor::parse(OptimizationIndexCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }

    fn validate(task: TaskId) -> Result<(), OptimizationIndexError> {
        if task.as_uuid().get_version_num() != 7
            || task.as_uuid().get_variant() != uuid::Variant::RFC4122
        {
            return Err(OptimizationIndexError);
        }
        Ok(())
    }
    pub fn collection(&self) -> OptimizationCollection {
        self.cursor.position().collection
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.cursor.position().created_at
    }
    pub fn task_id(&self) -> TaskId {
        self.cursor.position().task_id
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
impl TryFrom<String> for OptimizationIndexCursor {
    type Error = OptimizationIndexError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<OptimizationIndexCursor> for String {
    fn from(cursor: OptimizationIndexCursor) -> Self {
        cursor.cursor.into_wire()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct OptimizationCollectionUri {
    wire: ResourceUri,
    collection: OptimizationCollection,
    cursor: Option<OptimizationIndexCursor>,
}
impl OptimizationCollectionUri {
    pub fn new(
        collection: OptimizationCollection,
        cursor: Option<OptimizationIndexCursor>,
    ) -> Result<Self, OptimizationIndexError> {
        if cursor
            .as_ref()
            .is_some_and(|cursor| cursor.collection() != collection)
        {
            return Err(OptimizationIndexError);
        }
        let mut builder =
            ResourceUriBuilder::new(collection.root()).expect("declared collection root");
        if let Some(cursor) = &cursor {
            builder = builder
                .query_pair("cursor", cursor.as_str())
                .map_err(|_| OptimizationIndexError)?;
        }
        Ok(Self {
            wire: builder.build().map_err(|_| OptimizationIndexError)?,
            collection,
            cursor,
        })
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationIndexError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| OptimizationIndexError)?;
        if parts.scheme() != "optimization" || parts.path_segments().next().is_some() {
            return Err(OptimizationIndexError);
        }
        let collection = match parts.authority() {
            "problems" => OptimizationCollection::Problems,
            "runs" => OptimizationCollection::Runs,
            "solutions" => OptimizationCollection::Solutions,
            _ => return Err(OptimizationIndexError),
        };
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(OptimizationIndexError);
            }
            Some(OptimizationIndexCursor::parse(
                query.get("cursor").ok_or(OptimizationIndexError)?.clone(),
            )?)
        } else {
            None
        };
        let uri = Self::new(collection, cursor)?;
        if uri.as_str() != value {
            return Err(OptimizationIndexError);
        }
        Ok(uri)
    }
    pub fn collection(&self) -> OptimizationCollection {
        self.collection
    }
    pub fn cursor(&self) -> Option<&OptimizationIndexCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl TryFrom<String> for OptimizationCollectionUri {
    type Error = OptimizationIndexError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<OptimizationCollectionUri> for String {
    fn from(value: OptimizationCollectionUri) -> Self {
        value.wire.to_string()
    }
}
impl ResourceAddress for OptimizationCollectionUri {
    type Error = OptimizationIndexError;
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
}
