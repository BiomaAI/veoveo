//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.
use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, TaskId, UriSegment,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbUsageIndexUri {
    wire: ResourceUri,
    cursor: Option<DuckDbUsageCursor>,
}

impl DuckDbUsageIndexUri {
    pub const ROOT: &str = "duckdb://usage";
    pub const TEMPLATE: &str = "duckdb://usage{?cursor}";

    pub fn new(cursor: Option<&DuckDbUsageCursor>) -> Self {
        let mut builder = ResourceUriBuilder::new(Self::ROOT).expect("declared usage root");
        if let Some(cursor) = cursor {
            builder = builder
                .query_pair("cursor", cursor.as_str())
                .expect("typed usage cursor");
        }
        Self {
            wire: builder.build().expect("typed usage index URI"),
            cursor: cursor.cloned(),
        }
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, DuckDbUsageError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| DuckDbUsageError)?;
        if parts.scheme() != "duckdb"
            || parts.authority() != "usage"
            || parts.path_segments().next().is_some()
        {
            return Err(DuckDbUsageError);
        }
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(DuckDbUsageError);
            }
            Some(DuckDbUsageCursor::parse(
                query.get("cursor").ok_or(DuckDbUsageError)?.clone(),
            )?)
        } else {
            None
        };
        let uri = Self::new(cursor.as_ref());
        if uri.as_str() != value {
            return Err(DuckDbUsageError);
        }
        Ok(uri)
    }

    pub fn cursor(&self) -> Option<&DuckDbUsageCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbTaskUsageUri {
    wire: ResourceUri,
    task_id: TaskId,
}

impl DuckDbTaskUsageUri {
    pub const TEMPLATE: &str = "duckdb://usage/task/{task_id}";

    /// Database identities cannot be used as Task addresses.
    /// ```compile_fail
    /// use veoveo_duckdb_mcp::contract::{DuckDbDatabaseId, DuckDbTaskUsageUri};
    /// DuckDbTaskUsageUri::new(DuckDbDatabaseId::new("metrics").unwrap());
    /// ```
    pub fn new(task_id: TaskId) -> Result<Self, DuckDbUsageError> {
        let task_id = task_identity(task_id)?;
        let wire = ResourceUriBuilder::new(DuckDbUsageIndexUri::ROOT)
            .expect("declared usage root")
            .segment(UriSegment::new("task").expect("declared task segment"))
            .segment(UriSegment::new(task_id.to_string()).expect("canonical native Task UUID"))
            .build()
            .expect("typed Task usage URI");
        Ok(Self { wire, task_id })
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, DuckDbUsageError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| DuckDbUsageError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        if parts.scheme() != "duckdb"
            || parts.authority() != "usage"
            || parts.has_query()
            || path.len() != 2
            || path[0] != "task"
        {
            return Err(DuckDbUsageError);
        }
        let uri = Self::new(path[1].parse().map_err(|_| DuckDbUsageError)?)?;
        if uri.as_str() != value {
            return Err(DuckDbUsageError);
        }
        Ok(uri)
    }

    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

macro_rules! address_traits {
    ($name:ident) => {
        impl ResourceAddress for $name {
            type Error = DuckDbUsageError;
            fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(uri.as_str())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok(self.wire.clone())
            }
        }
        impl TryFrom<String> for $name {
            type Error = DuckDbUsageError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.wire.into()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
address_traits!(DuckDbUsageIndexUri);
address_traits!(DuckDbTaskUsageUri);

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
