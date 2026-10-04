//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.
use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri, TaskId};

pub const DUCKDB_USAGE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuckDbUsageError;

impl fmt::Display for DuckDbUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a valid DuckDB usage page, cursor, or canonical Task UUIDv7 address")
    }
}
impl std::error::Error for DuckDbUsageError {}

pub(super) fn task_identity(id: TaskId) -> Result<TaskId, DuckDbUsageError> {
    (id.as_uuid().get_version_num() == 7 && id.as_uuid().get_variant() == uuid::Variant::RFC4122)
        .then_some(id)
        .ok_or(DuckDbUsageError)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    collection: String,
    after: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbUsageCursor {
    wire: String,
    after: TaskId,
}

impl DuckDbUsageCursor {
    /// Positions require native Task identities.
    /// ```compile_fail
    /// use veoveo_duckdb_mcp::contract::DuckDbUsageCursor;
    /// DuckDbUsageCursor::new("raw-task-id");
    /// ```
    pub fn new(after: TaskId) -> Result<Self, DuckDbUsageError> {
        let after = task_identity(after)?;
        let wire = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&CursorWire {
                version: 1,
                collection: DuckDbUsageIndexUri::ROOT.to_owned(),
                after,
            })
            .expect("closed usage cursor fields serialize"),
        );
        Ok(Self { wire, after })
    }

    pub fn parse(wire: impl Into<String>) -> Result<Self, DuckDbUsageError> {
        let wire = wire.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(DuckDbUsageError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(&wire)
            .map_err(|_| DuckDbUsageError)?;
        let cursor: CursorWire = serde_json::from_slice(&bytes).map_err(|_| DuckDbUsageError)?;
        if cursor.version != 1 || cursor.collection != DuckDbUsageIndexUri::ROOT {
            return Err(DuckDbUsageError);
        }
        Ok(Self {
            wire,
            after: task_identity(cursor.after)?,
        })
    }

    pub fn after(&self) -> TaskId {
        self.after
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}

impl TryFrom<String> for DuckDbUsageCursor {
    type Error = DuckDbUsageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<DuckDbUsageCursor> for String {
    fn from(value: DuckDbUsageCursor) -> Self {
        value.wire
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="duckdb://usage{?cursor}", error=DuckDbUsageError, route_error=|_| DuckDbUsageError, wire)]
pub struct DuckDbUsageIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageCursorCodec, error=|_| DuckDbUsageError)]
    cursor: Option<DuckDbUsageCursor>,
}

impl DuckDbUsageIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(cursor: Option<&DuckDbUsageCursor>) -> Self {
        Self::resource_from_parts(cursor.cloned()).expect("typed usage index address")
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, DuckDbUsageError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| DuckDbUsageError)?;
        <Self as ResourceAddress>::parse(&uri)
    }

    pub fn cursor(&self) -> Option<&DuckDbUsageCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="duckdb://usage/task/{task_id}", error=DuckDbUsageError, route_error=|_| DuckDbUsageError, wire)]
pub struct DuckDbTaskUsageUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageTaskCodec, error=|_| DuckDbUsageError)]
    task_id: TaskId,
}

impl DuckDbTaskUsageUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    /// Database identities cannot be used as Task addresses.
    /// ```compile_fail
    /// use veoveo_duckdb_mcp::contract::{DuckDbDatabaseId, DuckDbTaskUsageUri};
    /// DuckDbTaskUsageUri::new(DuckDbDatabaseId::new("metrics").unwrap());
    /// ```
    pub fn new(task_id: TaskId) -> Result<Self, DuckDbUsageError> {
        Self::resource_from_parts(task_identity(task_id)?)
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, DuckDbUsageError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| DuckDbUsageError)?;
        <Self as ResourceAddress>::parse(&uri)
    }

    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

struct UsageCursorCodec;
impl ResourceFieldCodec<DuckDbUsageCursor> for UsageCursorCodec {
    type Error = DuckDbUsageError;
    fn parse(value: &str) -> Result<DuckDbUsageCursor, Self::Error> {
        DuckDbUsageCursor::parse(value)
    }
    fn text(value: &DuckDbUsageCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
struct UsageTaskCodec;
impl ResourceFieldCodec<TaskId> for UsageTaskCodec {
    type Error = DuckDbUsageError;
    fn parse(value: &str) -> Result<TaskId, Self::Error> {
        task_identity(value.parse().map_err(|_| DuckDbUsageError)?)
    }
    fn text(value: &TaskId) -> std::borrow::Cow<'_, str> {
        value.to_string().into()
    }
}
impl fmt::Display for DuckDbUsageIndexUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl fmt::Display for DuckDbTaskUsageUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EntryWire", into = "EntryWire")]
pub struct DuckDbUsageEntry {
    usage_uri: DuckDbTaskUsageUri,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    task_id: TaskId,
    usage_uri: DuckDbTaskUsageUri,
}

impl DuckDbUsageEntry {
    pub fn new(task_id: TaskId) -> Result<Self, DuckDbUsageError> {
        Ok(Self {
            usage_uri: DuckDbTaskUsageUri::new(task_id)?,
        })
    }
    pub fn task_id(&self) -> TaskId {
        self.usage_uri.task_id()
    }
    pub fn usage_uri(&self) -> &DuckDbTaskUsageUri {
        &self.usage_uri
    }
}
impl TryFrom<EntryWire> for DuckDbUsageEntry {
    type Error = DuckDbUsageError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        if value.task_id != value.usage_uri.task_id() {
            return Err(DuckDbUsageError);
        }
        Ok(Self {
            usage_uri: value.usage_uri,
        })
    }
}
impl From<DuckDbUsageEntry> for EntryWire {
    fn from(value: DuckDbUsageEntry) -> Self {
        Self {
            task_id: value.task_id(),
            usage_uri: value.usage_uri,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct DuckDbUsagePage {
    items: Vec<DuckDbUsageEntry>,
    next_cursor: Option<DuckDbUsageCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    items: Vec<DuckDbUsageEntry>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    next_cursor: Option<DuckDbUsageCursor>,
}

impl DuckDbUsagePage {
    pub fn from_task_ids(ids: Vec<TaskId>, next: Option<TaskId>) -> Result<Self, DuckDbUsageError> {
        Self::try_from(PageWire {
            items: ids
                .into_iter()
                .map(DuckDbUsageEntry::new)
                .collect::<Result<_, _>>()?,
            limit: DUCKDB_USAGE_PAGE_SIZE,
            next_cursor: next.map(DuckDbUsageCursor::new).transpose()?,
        })
    }

    pub fn items(&self) -> &[DuckDbUsageEntry] {
        &self.items
    }
    pub fn next_cursor(&self) -> Option<&DuckDbUsageCursor> {
        self.next_cursor.as_ref()
    }
}
impl TryFrom<PageWire> for DuckDbUsagePage {
    type Error = DuckDbUsageError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        if value.limit != DUCKDB_USAGE_PAGE_SIZE
            || value.items.len() > DUCKDB_USAGE_PAGE_SIZE
            || value
                .items
                .windows(2)
                .any(|pair| pair[0].task_id() >= pair[1].task_id())
            || value.next_cursor.as_ref().is_some_and(|cursor| {
                value.items.len() != DUCKDB_USAGE_PAGE_SIZE
                    || value.items.last().map(DuckDbUsageEntry::task_id) != Some(cursor.after())
            })
        {
            return Err(DuckDbUsageError);
        }
        Ok(Self {
            items: value.items,
            next_cursor: value.next_cursor,
        })
    }
}
impl From<DuckDbUsagePage> for PageWire {
    fn from(value: DuckDbUsagePage) -> Self {
        Self {
            items: value.items,
            limit: DUCKDB_USAGE_PAGE_SIZE,
            next_cursor: value.next_cursor,
        }
    }
}
