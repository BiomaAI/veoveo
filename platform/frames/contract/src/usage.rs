//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri, TaskId};

pub const FRAME_USAGE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameUsageError;

impl fmt::Display for FrameUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a valid Frames usage page, cursor, or canonical Task UUIDv7 address")
    }
}
impl std::error::Error for FrameUsageError {}

fn task_identity(id: TaskId) -> Result<TaskId, FrameUsageError> {
    (id.as_uuid().get_version_num() == 7)
        .then_some(id)
        .ok_or(FrameUsageError)
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
pub struct FrameUsageCursor {
    wire: String,
    after: TaskId,
}

impl FrameUsageCursor {
    /// Positions require native Tasks, never frame-world or operation IDs.
    /// ```compile_fail
    /// use veoveo_frames_contract::{FrameUsageCursor, FrameWorldId};
    /// FrameUsageCursor::new(FrameWorldId::new("world").unwrap());
    /// ```
    pub fn new(after: TaskId) -> Result<Self, FrameUsageError> {
        let after = task_identity(after)?;
        let wire = hex::encode(
            serde_json::to_vec(&CursorWire {
                version: 1,
                collection: FrameUsageIndexUri::ROOT.to_owned(),
                after,
            })
            .expect("closed usage cursor fields serialize"),
        );
        Ok(Self { wire, after })
    }

    pub fn parse(wire: impl Into<String>) -> Result<Self, FrameUsageError> {
        let wire = wire.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(FrameUsageError);
        }
        let bytes = hex::decode(&wire).map_err(|_| FrameUsageError)?;
        let cursor: CursorWire = serde_json::from_slice(&bytes).map_err(|_| FrameUsageError)?;
        if cursor.version != 1 || cursor.collection != FrameUsageIndexUri::ROOT {
            return Err(FrameUsageError);
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

impl TryFrom<String> for FrameUsageCursor {
    type Error = FrameUsageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<FrameUsageCursor> for String {
    fn from(value: FrameUsageCursor) -> Self {
        value.wire
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="frames://usage{?cursor}", error=FrameUsageError, route_error=|_| FrameUsageError, wire)]
pub struct FrameUsageIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageCursorCodec, error=|_| FrameUsageError)]
    cursor: Option<FrameUsageCursor>,
}

impl FrameUsageIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(cursor: Option<&FrameUsageCursor>) -> Self {
        Self::resource_from_parts(cursor.cloned()).expect("typed usage index address")
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, FrameUsageError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| FrameUsageError)?;
        <Self as ResourceAddress>::parse(&uri)
    }

    pub fn cursor(&self) -> Option<&FrameUsageCursor> {
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
#[resource(template="frames://usage/task/{task_id}", error=FrameUsageError, route_error=|_| FrameUsageError, wire)]
pub struct FrameTaskUsageUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageTaskCodec, error=|_| FrameUsageError)]
    task_id: TaskId,
}

impl FrameTaskUsageUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(task_id: TaskId) -> Result<Self, FrameUsageError> {
        Self::resource_from_parts(task_identity(task_id)?)
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, FrameUsageError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| FrameUsageError)?;
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
impl ResourceFieldCodec<FrameUsageCursor> for UsageCursorCodec {
    type Error = FrameUsageError;
    fn parse(value: &str) -> Result<FrameUsageCursor, Self::Error> {
        FrameUsageCursor::parse(value)
    }
    fn text(value: &FrameUsageCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
struct UsageTaskCodec;
impl ResourceFieldCodec<TaskId> for UsageTaskCodec {
    type Error = FrameUsageError;
    fn parse(value: &str) -> Result<TaskId, Self::Error> {
        task_identity(value.parse().map_err(|_| FrameUsageError)?)
    }
    fn text(value: &TaskId) -> std::borrow::Cow<'_, str> {
        value.to_string().into()
    }
}
impl fmt::Display for FrameUsageIndexUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl fmt::Display for FrameTaskUsageUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EntryWire", into = "EntryWire")]
pub struct FrameUsageEntry {
    usage_uri: FrameTaskUsageUri,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    task_id: TaskId,
    usage_uri: FrameTaskUsageUri,
}

impl FrameUsageEntry {
    pub fn new(task_id: TaskId) -> Result<Self, FrameUsageError> {
        Ok(Self {
            usage_uri: FrameTaskUsageUri::new(task_id)?,
        })
    }
    pub fn task_id(&self) -> TaskId {
        self.usage_uri.task_id()
    }
    pub fn usage_uri(&self) -> &FrameTaskUsageUri {
        &self.usage_uri
    }
}
impl TryFrom<EntryWire> for FrameUsageEntry {
    type Error = FrameUsageError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        if value.task_id != value.usage_uri.task_id() {
            return Err(FrameUsageError);
        }
        Ok(Self {
            usage_uri: value.usage_uri,
        })
    }
}
impl From<FrameUsageEntry> for EntryWire {
    fn from(value: FrameUsageEntry) -> Self {
        Self {
            task_id: value.task_id(),
            usage_uri: value.usage_uri,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct FrameUsagePage {
    items: Vec<FrameUsageEntry>,
    next_cursor: Option<FrameUsageCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    items: Vec<FrameUsageEntry>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    next_cursor: Option<FrameUsageCursor>,
}

impl FrameUsagePage {
    pub fn from_task_ids(ids: Vec<TaskId>, next: Option<TaskId>) -> Result<Self, FrameUsageError> {
        Self::try_from(PageWire {
            items: ids
                .into_iter()
                .map(FrameUsageEntry::new)
                .collect::<Result<_, _>>()?,
            limit: FRAME_USAGE_PAGE_SIZE,
            next_cursor: next.map(FrameUsageCursor::new).transpose()?,
        })
    }

    pub fn items(&self) -> &[FrameUsageEntry] {
        &self.items
    }
    pub fn next_cursor(&self) -> Option<&FrameUsageCursor> {
        self.next_cursor.as_ref()
    }
}
impl TryFrom<PageWire> for FrameUsagePage {
    type Error = FrameUsageError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        if value.limit != FRAME_USAGE_PAGE_SIZE
            || value.items.len() > FRAME_USAGE_PAGE_SIZE
            || value
                .items
                .windows(2)
                .any(|pair| pair[0].task_id() >= pair[1].task_id())
            || value.next_cursor.as_ref().is_some_and(|cursor| {
                value.items.len() != FRAME_USAGE_PAGE_SIZE
                    || value.items.last().map(FrameUsageEntry::task_id) != Some(cursor.after())
            })
        {
            return Err(FrameUsageError);
        }
        Ok(Self {
            items: value.items,
            next_cursor: value.next_cursor,
        })
    }
}
impl From<FrameUsagePage> for PageWire {
    fn from(value: FrameUsagePage) -> Self {
        Self {
            items: value.items,
            limit: FRAME_USAGE_PAGE_SIZE,
            next_cursor: value.next_cursor,
        }
    }
}
