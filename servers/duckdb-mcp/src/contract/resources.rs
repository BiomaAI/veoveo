//! DuckDB resource admission composed with foundational URI components.
use super::{DuckDbDatabaseCursor, DuckDbDatabaseId, DuckDbTaskUsageUri, DuckDbUsageIndexUri};
use crate::uris;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("expected a canonical DuckDB resource with its declared parameters")]
pub struct DuckDbResourceError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuckDbDocument {
    Agents,
    Design,
}
impl DuckDbDocument {
    pub fn parse(value: &str) -> Result<Self, DuckDbResourceError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(DuckDbResourceError),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbDatabaseUri(DuckDbDatabaseId);
impl DuckDbDatabaseUri {
    /// ```compile_fail
    /// use veoveo_duckdb_mcp::contract::DuckDbDatabaseUri;
    /// DuckDbDatabaseUri::new("unadmitted-name");
    /// ```
    pub fn new(id: DuckDbDatabaseId) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &DuckDbDatabaseId {
        &self.0
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DuckDbResourceError> {
        match DuckDbResource::parse(value)? {
            DuckDbResource::Database(uri) => Ok(uri),
            _ => Err(DuckDbResourceError),
        }
    }
    pub fn to_uri(&self) -> ResourceUri {
        ResourceUriBuilder::new("duckdb://db")
            .expect("declared database root")
            .segment(UriSegment::new(self.0.as_str()).expect("admitted database ID"))
            .build()
            .expect("admitted database URI")
    }
}
impl ResourceAddress for DuckDbDatabaseUri {
    type Error = DuckDbResourceError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.to_uri())
    }
}
impl TryFrom<String> for DuckDbDatabaseUri {
    type Error = DuckDbResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<DuckDbDatabaseUri> for String {
    fn from(value: DuckDbDatabaseUri) -> Self {
        value.to_uri().to_string()
    }
}
impl fmt::Display for DuckDbDatabaseUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub enum DuckDbResource {
    Docs,
    Document(DuckDbDocument),
    Contract,
    Workbench,
    Databases(Option<DuckDbDatabaseCursor>),
    Database(DuckDbDatabaseUri),
    Usage(DuckDbUsageIndexUri),
    TaskUsage(DuckDbTaskUsageUri),
    Artifact(ArtifactId),
}
impl DuckDbResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DuckDbResourceError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| DuckDbResourceError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let path = path.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), path.as_slice()) {
            ("duckdb", "dbs", []) => {
                let cursor = if parts.has_query() {
                    let query = parts.query_parameters();
                    if query.len() != 1 {
                        return Err(DuckDbResourceError);
                    }
                    Some(
                        DuckDbDatabaseCursor::parse(
                            query.get("cursor").ok_or(DuckDbResourceError)?.clone(),
                        )
                        .map_err(|_| DuckDbResourceError)?,
                    )
                } else {
                    None
                };
                Self::Databases(cursor)
            }
            ("duckdb", "usage", []) => {
                Self::Usage(DuckDbUsageIndexUri::parse(value).map_err(|_| DuckDbResourceError)?)
            }
            _ if parts.has_query() => return Err(DuckDbResourceError),
            ("duckdb", "docs", []) => Self::Docs,
            ("duckdb", "docs", [id]) => Self::Document(DuckDbDocument::parse(id)?),
            ("duckdb", "contract", []) => Self::Contract,
            ("ui", "duckdb", ["workbench.html"]) => Self::Workbench,
            ("duckdb", "db", [id]) => Self::Database(DuckDbDatabaseUri::new(
                DuckDbDatabaseId::new(*id).map_err(|_| DuckDbResourceError)?,
            )),
            ("duckdb", "usage", ["task", _]) => {
                Self::TaskUsage(DuckDbTaskUsageUri::parse(value).map_err(|_| DuckDbResourceError)?)
            }
            ("duckdb", "artifact", [_]) => Self::Artifact(
                ArtifactUri::parse(value)
                    .map_err(|_| DuckDbResourceError)?
                    .artifact_id(),
            ),
            _ => return Err(DuckDbResourceError),
        };
        if resource.to_uri()?.as_str() != value {
            return Err(DuckDbResourceError);
        }
        Ok(resource)
    }
}
impl ResourceAddress for DuckDbResource {
    type Error = DuckDbResourceError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let literal = |s| ResourceUri::new(s).map_err(|_| DuckDbResourceError);
        match self {
            Self::Docs => literal(uris::DOCS_URI),
            Self::Contract => literal(uris::CONTRACT_URI),
            Self::Workbench => literal(uris::WORKBENCH_APP_URI),
            Self::Document(id) => ResourceUriBuilder::new(uris::DOCS_URI)
                .map_err(|_| DuckDbResourceError)?
                .segment(UriSegment::new(id.as_str()).map_err(|_| DuckDbResourceError)?)
                .build()
                .map_err(|_| DuckDbResourceError),
            Self::Databases(cursor) => {
                let mut builder =
                    ResourceUriBuilder::new(uris::DBS_ROOT_URI).map_err(|_| DuckDbResourceError)?;
                if let Some(cursor) = cursor {
                    builder = builder
                        .query_pair("cursor", cursor.as_str())
                        .map_err(|_| DuckDbResourceError)?;
                }
                builder.build().map_err(|_| DuckDbResourceError)
            }
            Self::Database(uri) => Ok(uri.to_uri()),
            Self::Usage(uri) => uri.to_uri().map_err(|_| DuckDbResourceError),
            Self::TaskUsage(uri) => uri.to_uri().map_err(|_| DuckDbResourceError),
            Self::Artifact(id) => ArtifactUri::presented(&uris::SCHEME, *id)
                .to_uri()
                .map_err(|_| DuckDbResourceError),
        }
    }
}
impl TryFrom<String> for DuckDbResource {
    type Error = DuckDbResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<DuckDbResource> for String {
    fn from(value: DuckDbResource) -> Self {
        value
            .to_uri()
            .expect("admitted DuckDB resource")
            .to_string()
    }
}
