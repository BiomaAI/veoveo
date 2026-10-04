//! Owner-local database pages. A cursor grants no access to another workspace.
use super::{DuckDbDatabaseId, DuckDbDatabaseUri};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DUCKDB_DATABASE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("expected an ordered DuckDB database page with matching identities and cursor")]
pub struct DuckDbCatalogError;

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
    cursor: veoveo_types::OpaqueCursor<DuckDbDatabaseCursorCodec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DuckDbDatabaseCursorCodec;
impl veoveo_types::CursorCodec for DuckDbDatabaseCursorCodec {
    type Position = DuckDbDatabaseId;
    type Error = DuckDbCatalogError;
    fn check(&self, _position: &Self::Position) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 1,
            collection: (crate::uris::DBS_ROOT_URI).to_owned(),
            after: position.clone(),
        })
        .expect("closed owner cursor fields serialize");
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(DuckDbCatalogError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| DuckDbCatalogError)?;
        let decoded: CursorWire = serde_json::from_slice(&bytes).map_err(|_| DuckDbCatalogError)?;
        if decoded.version != 1 || decoded.collection != crate::uris::DBS_ROOT_URI {
            return Err(DuckDbCatalogError);
        }
        let position = decoded.after;
        self.check(&position)?;
        if self.encode(&position)? != wire {
            return Err(DuckDbCatalogError);
        }
        Ok(position)
    }
}
impl DuckDbDatabaseCursor {
    pub fn new(after: DuckDbDatabaseId) -> Self {
        let cursor = veoveo_types::OpaqueCursor::try_new(DuckDbDatabaseCursorCodec, after)
            .expect("typed cursor position");
        Self { cursor }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, DuckDbCatalogError> {
        veoveo_types::OpaqueCursor::parse(DuckDbDatabaseCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }
    pub fn after(&self) -> &DuckDbDatabaseId {
        self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
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
        value.cursor.into_wire()
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
impl veoveo_types::Check for EntryWire {
    type Error = DuckDbCatalogError;
    fn check(&self) -> Result<(), Self::Error> {
        if &self.db_id != self.db_uri.id() {
            return Err(DuckDbCatalogError);
        }

        Ok(())
    }
}
impl TryFrom<EntryWire> for DuckDbDatabaseEntry {
    type Error = DuckDbCatalogError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
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
impl veoveo_types::Check for PageWire {
    type Error = DuckDbCatalogError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != DUCKDB_DATABASE_PAGE_SIZE
            || self.items.len() > DUCKDB_DATABASE_PAGE_SIZE
            || self
                .items
                .windows(2)
                .any(|pair| pair[0].id() >= pair[1].id())
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.items.len() != DUCKDB_DATABASE_PAGE_SIZE
                    || self
                        .items
                        .last()
                        .is_none_or(|entry| entry.id() != cursor.after())
            })
        {
            return Err(DuckDbCatalogError);
        }

        Ok(())
    }
}
impl TryFrom<PageWire> for DuckDbDatabasePage {
    type Error = DuckDbCatalogError;
    fn try_from(wire: PageWire) -> Result<Self, Self::Error> {
        let wire = veoveo_types::Checked::new(wire)?.into_inner();
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
