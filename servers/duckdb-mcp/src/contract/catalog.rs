//! Owner-local database pages. A cursor grants no access to another workspace.
use super::{DuckDbDatabaseId, DuckDbDatabaseUri};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const DUCKDB_DATABASE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DuckDbCatalogError;
impl fmt::Display for DuckDbCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected an ordered DuckDB database page with matching identities and cursor")
    }
}
impl std::error::Error for DuckDbCatalogError {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    collection: String,
    after: DuckDbDatabaseId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbDatabaseCursor {
    wire: String,
    after: DuckDbDatabaseId,
}
impl DuckDbDatabaseCursor {
    pub fn new(after: DuckDbDatabaseId) -> Self {
        let wire = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&CursorWire {
                version: 1,
                collection: crate::uris::DBS_ROOT_URI.into(),
                after: after.clone(),
            })
            .expect("closed database cursor fields serialize"),
        );
        Self { wire, after }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, DuckDbCatalogError> {
        let wire = wire.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(DuckDbCatalogError);
        }
        let decoded = URL_SAFE_NO_PAD
            .decode(&wire)
            .map_err(|_| DuckDbCatalogError)?;
        let fields: CursorWire =
            serde_json::from_slice(&decoded).map_err(|_| DuckDbCatalogError)?;
        if fields.version != 1 || fields.collection != crate::uris::DBS_ROOT_URI {
            return Err(DuckDbCatalogError);
        }
        let cursor = Self::new(fields.after);
        if cursor.wire != wire {
            return Err(DuckDbCatalogError);
        }
        Ok(cursor)
    }
    pub fn after(&self) -> &DuckDbDatabaseId {
        &self.after
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}
impl TryFrom<String> for DuckDbDatabaseCursor {
    type Error = DuckDbCatalogError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<DuckDbDatabaseCursor> for String {
    fn from(value: DuckDbDatabaseCursor) -> Self {
        value.wire
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EntryWire", into = "EntryWire")]
pub struct DuckDbDatabaseEntry {
    uri: DuckDbDatabaseUri,
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    db_id: DuckDbDatabaseId,
    db_uri: DuckDbDatabaseUri,
}
impl DuckDbDatabaseEntry {
    pub fn new(id: DuckDbDatabaseId) -> Self {
        Self {
            uri: DuckDbDatabaseUri::new(id),
        }
    }
    pub fn id(&self) -> &DuckDbDatabaseId {
        self.uri.id()
    }
    pub fn uri(&self) -> &DuckDbDatabaseUri {
        &self.uri
    }
}
impl TryFrom<EntryWire> for DuckDbDatabaseEntry {
    type Error = DuckDbCatalogError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        if &value.db_id != value.db_uri.id() {
            return Err(DuckDbCatalogError);
        }
        Ok(Self { uri: value.db_uri })
    }
}
impl From<DuckDbDatabaseEntry> for EntryWire {
    fn from(value: DuckDbDatabaseEntry) -> Self {
        Self {
            db_id: value.id().clone(),
            db_uri: value.uri,
        }
    }
}

/// An immutable ascending page. Only the caller's current workspace supplies its items.
/// ```compile_fail
/// use veoveo_duckdb_mcp::contract::DuckDbDatabasePage;
/// fn mutate(page: &mut DuckDbDatabasePage) { page.items().clear(); }
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
#[schemars(
    description = "An immutable ascending page from the caller's current database workspace."
)]
pub struct DuckDbDatabasePage {
    items: Vec<DuckDbDatabaseEntry>,
    next_cursor: Option<DuckDbDatabaseCursor>,
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    items: Vec<DuckDbDatabaseEntry>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    next_cursor: Option<DuckDbDatabaseCursor>,
}
impl DuckDbDatabasePage {
    pub fn from_ids(
        ids: Vec<DuckDbDatabaseId>,
        has_more: bool,
    ) -> Result<Self, DuckDbCatalogError> {
        let next_cursor = if has_more {
            Some(DuckDbDatabaseCursor::new(
                ids.last().ok_or(DuckDbCatalogError)?.clone(),
            ))
        } else {
            None
        };
        Self::try_from(PageWire {
            items: ids.into_iter().map(DuckDbDatabaseEntry::new).collect(),
            limit: DUCKDB_DATABASE_PAGE_SIZE,
            next_cursor,
        })
    }
    pub fn items(&self) -> &[DuckDbDatabaseEntry] {
        &self.items
    }
    pub fn next_cursor(&self) -> Option<&DuckDbDatabaseCursor> {
        self.next_cursor.as_ref()
    }
}
impl TryFrom<PageWire> for DuckDbDatabasePage {
    type Error = DuckDbCatalogError;
    fn try_from(wire: PageWire) -> Result<Self, Self::Error> {
        if wire.limit != DUCKDB_DATABASE_PAGE_SIZE
            || wire.items.len() > DUCKDB_DATABASE_PAGE_SIZE
            || wire
                .items
                .windows(2)
                .any(|pair| pair[0].id() >= pair[1].id())
            || wire.next_cursor.as_ref().is_some_and(|cursor| {
                wire.items.len() != DUCKDB_DATABASE_PAGE_SIZE
                    || wire
                        .items
                        .last()
                        .is_none_or(|entry| entry.id() != cursor.after())
            })
        {
            return Err(DuckDbCatalogError);
        }
        Ok(Self {
            items: wire.items,
            next_cursor: wire.next_cursor,
        })
    }
}
impl From<DuckDbDatabasePage> for PageWire {
    fn from(value: DuckDbDatabasePage) -> Self {
        Self {
            items: value.items,
            limit: DUCKDB_DATABASE_PAGE_SIZE,
            next_cursor: value.next_cursor,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuckDbDatabaseSchema {
    pub db_id: DuckDbDatabaseId,
    pub tables: Vec<DuckDbTableSchema>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuckDbTableSchema {
    pub name: String,
    pub columns: Vec<DuckDbSchemaColumn>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuckDbSchemaColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
}
