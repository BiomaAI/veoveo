//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceFieldCodec, ResourceUri, TaskId};

pub const DUCKDB_USAGE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("expected a valid DuckDB usage page, cursor, or canonical Task UUIDv7 address")]
pub struct DuckDbUsageError;

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
#[serde(transparent)]
pub struct DuckDbUsageCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<DuckDbUsageCursorCodec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DuckDbUsageCursorCodec;
impl veoveo_types::StatelessCursorCodec for DuckDbUsageCursorCodec {}
impl veoveo_types::CursorCodec for DuckDbUsageCursorCodec {
    type Position = TaskId;
    type Error = DuckDbUsageError;
    fn check(&self, position: &Self::Position) -> Result<(), Self::Error> {
        task_identity(*position).map(|_| ())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 1,
            collection: (DuckDbUsageIndexUri::ROOT).to_owned(),
            after: *position,
        })
        .expect("closed owner cursor fields serialize");
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(DuckDbUsageError);
        }
        let bytes = URL_SAFE_NO_PAD.decode(wire).map_err(|_| DuckDbUsageError)?;
        let decoded: CursorWire = serde_json::from_slice(&bytes).map_err(|_| DuckDbUsageError)?;
        if decoded.version != 1 || decoded.collection != DuckDbUsageIndexUri::ROOT {
            return Err(DuckDbUsageError);
        }
        let position = decoded.after;
        Ok(position)
    }
}
impl DuckDbUsageCursor {
    /// Positions require native Task identities.
    /// ```compile_fail
    /// use veoveo_duckdb_mcp::contract::DuckDbUsageCursor;
    /// DuckDbUsageCursor::new("raw-task-id");
    /// ```
    pub fn new(after: TaskId) -> Result<Self, DuckDbUsageError> {
        let cursor = veoveo_types::OpaqueCursor::try_new(DuckDbUsageCursorCodec, after)?;
        Ok(Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, DuckDbUsageError> {
        veoveo_types::OpaqueCursor::parse(DuckDbUsageCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }
    pub fn after(&self) -> TaskId {
        *self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
#[veoveo_types::resource_address(
    cached(DuckDbUsageErrorAddresses),
    template = "duckdb://usage{?cursor}"
)]
pub struct DuckDbUsageIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageCursorCodec, error=|_| DuckDbUsageError, accessor = cursor, argument = optional_borrowed)]
    cursor: Option<DuckDbUsageCursor>,
}

impl DuckDbUsageIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached_checked(DuckDbUsageErrorAddresses), template = "duckdb://usage/task/{task_id}", traits = cloned, schema = derived)]
pub struct DuckDbTaskUsageUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageTaskCodec, error=|_| DuckDbUsageError, accessor = task_id, copy_accessor, admit = task_identity)]
    task_id: TaskId,
}

impl DuckDbTaskUsageUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
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
impl veoveo_types::Check for EntryWire {
    type Error = DuckDbUsageError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.task_id != self.usage_uri.task_id() {
            return Err(DuckDbUsageError);
        }

        Ok(())
    }
}
impl TryFrom<EntryWire> for DuckDbUsageEntry {
    type Error = DuckDbUsageError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
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
impl veoveo_types::Check for PageWire {
    type Error = DuckDbUsageError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != DUCKDB_USAGE_PAGE_SIZE
            || self.items.len() > DUCKDB_USAGE_PAGE_SIZE
            || self
                .items
                .windows(2)
                .any(|pair| pair[0].task_id() >= pair[1].task_id())
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.items.len() != DUCKDB_USAGE_PAGE_SIZE
                    || self.items.last().map(DuckDbUsageEntry::task_id) != Some(cursor.after())
            })
        {
            return Err(DuckDbUsageError);
        }

        Ok(())
    }
}
impl TryFrom<PageWire> for DuckDbUsagePage {
    type Error = DuckDbUsageError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
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

#[doc(hidden)]
pub struct DuckDbUsageErrorAddresses;
impl veoveo_types::ResourceProfile for DuckDbUsageErrorAddresses {
    type Error = DuckDbUsageError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| DuckDbUsageError,
        };
}

/// Database identities cannot be used as Task addresses.
/// ```compile_fail
/// use veoveo_duckdb_mcp::contract::{DuckDbDatabaseId, DuckDbTaskUsageUri};
/// DuckDbTaskUsageUri::new(DuckDbDatabaseId::parse("metrics").unwrap());
/// ```
const _: () = ();
