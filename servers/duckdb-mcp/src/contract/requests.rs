//! Request construction checks relationships before file access or Task admission.
use super::{
    DuckDbDatabaseId, DuckDbQueryOutputMode, DuckDbSource, DuckDbSqlText, DuckDbTableName,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, num::NonZeroU64};

/// Query attachments and output options are checked together.
/// ```compile_fail
/// use veoveo_duckdb_mcp::contract::{DuckDbQueryRequest, DuckDbTableName};
/// DuckDbQueryRequest::builder("metrics".parse().unwrap(), DuckDbTableName::new("facts").unwrap());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "QueryWire", into = "QueryWire")]
#[schemars(transform = query_relationships)]
pub struct DuckDbQueryRequest(veoveo_types::Checked<QueryWire>);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct QueryWire {
    db: DuckDbDatabaseId,
    sql: DuckDbSqlText,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(transform = unique_attachments)]
    attach: Vec<DuckDbDatabaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    row_limit: Option<NonZeroU64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<NonZeroU64>,
    #[serde(default)]
    output: DuckDbQueryOutputMode,
}

impl DuckDbQueryRequest {
    pub fn builder(db: DuckDbDatabaseId, sql: DuckDbSqlText) -> DuckDbQueryRequestBuilder {
        DuckDbQueryRequestBuilder(QueryWire {
            db,
            sql,
            attach: vec![],
            row_limit: None,
            timeout_ms: None,
            output: DuckDbQueryOutputMode::default(),
        })
    }
    pub fn database(&self) -> &DuckDbDatabaseId {
        &self.0.db
    }
    pub fn sql(&self) -> &DuckDbSqlText {
        &self.0.sql
    }
    pub fn attachments(&self) -> &[DuckDbDatabaseId] {
        &self.0.attach
    }
    pub fn row_limit(&self) -> Option<NonZeroU64> {
        self.0.row_limit
    }
    pub fn timeout_ms(&self) -> Option<NonZeroU64> {
        self.0.timeout_ms
    }
    pub fn output(&self) -> &DuckDbQueryOutputMode {
        &self.0.output
    }
}
#[derive(Debug, Clone)]
pub struct DuckDbQueryRequestBuilder(QueryWire);
impl DuckDbQueryRequestBuilder {
    pub fn attach(mut self, databases: impl IntoIterator<Item = DuckDbDatabaseId>) -> Self {
        self.0.attach = databases.into_iter().collect();
        self
    }
    pub fn row_limit(mut self, limit: NonZeroU64) -> Self {
        self.0.row_limit = Some(limit);
        self
    }
    pub fn timeout_ms(mut self, timeout: NonZeroU64) -> Self {
        self.0.timeout_ms = Some(timeout);
        self
    }
    pub fn output(mut self, output: DuckDbQueryOutputMode) -> Self {
        self.0.output = output;
        self
    }
    pub fn build(self) -> Result<DuckDbQueryRequest, DuckDbQueryRequestError> {
        self.0.try_into()
    }
}
impl veoveo_types::Check for QueryWire {
    type Error = DuckDbQueryRequestError;
    fn check(&self) -> Result<(), Self::Error> {
        let wire = self;
        let mut seen = BTreeSet::new();
        for db in &wire.attach {
            if db == &wire.db {
                return Err(DuckDbQueryRequestError::MainDatabaseAttachment);
            }
            if !seen.insert(db) {
                return Err(DuckDbQueryRequestError::DuplicateAttachment);
            }
        }
        if wire.row_limit.is_some() && matches!(wire.output, DuckDbQueryOutputMode::Artifact { .. })
        {
            return Err(DuckDbQueryRequestError::ArtifactRowLimit);
        }
        Ok(())
    }
}
impl TryFrom<QueryWire> for DuckDbQueryRequest {
    type Error = DuckDbQueryRequestError;
    fn try_from(value: QueryWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<DuckDbQueryRequest> for QueryWire {
    fn from(value: DuckDbQueryRequest) -> Self {
        value.0.into_inner()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuckDbQueryRequestError {
    MainDatabaseAttachment,
    DuplicateAttachment,
    ArtifactRowLimit,
}
impl fmt::Display for DuckDbQueryRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MainDatabaseAttachment => "the query database cannot also be attached",
            Self::DuplicateAttachment => "query attachments must name distinct databases",
            Self::ArtifactRowLimit => {
                "row_limit applies only to inline query output; use SQL LIMIT for an Artifact query"
            }
        })
    }
}
impl std::error::Error for DuckDbQueryRequestError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct DuckDbExecuteRequest {
    pub db: DuckDbDatabaseId,
    pub sql: DuckDbSqlText,
    #[serde(default)]
    pub create_if_missing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<NonZeroU64>,
}
impl DuckDbExecuteRequest {
    pub fn new(db: DuckDbDatabaseId, sql: DuckDbSqlText) -> Self {
        Self {
            db,
            sql,
            create_if_missing: false,
            timeout_ms: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DuckDbIngestMode {
    Create,
    Append,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct DuckDbIngestRequest {
    pub db: DuckDbDatabaseId,
    pub table: DuckDbTableName,
    pub source: DuckDbSource,
    pub mode: DuckDbIngestMode,
    #[serde(default)]
    pub create_db_if_missing: bool,
}
impl DuckDbIngestRequest {
    pub fn new(
        db: DuckDbDatabaseId,
        table: DuckDbTableName,
        source: DuckDbSource,
        mode: DuckDbIngestMode,
    ) -> Self {
        Self {
            db,
            table,
            source,
            mode,
            create_db_if_missing: false,
        }
    }
}

fn unique_attachments(schema: &mut schemars::Schema) {
    schema.insert("uniqueItems".into(), true.into());
}

fn query_relationships(schema: &mut schemars::Schema) {
    schema.insert("allOf".into(), serde_json::json!([{
        "if":{"properties":{"output":{"properties":{"mode":{"const":"artifact"}},"required":["mode"]}},"required":["output"]},
        "then":{"properties":{"rowLimit":{"type":"null"}}}
    }]));
}
