//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.
use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri, TaskId};

pub const MEDIA_USAGE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaUsageError;

impl fmt::Display for MediaUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a valid Media usage page, cursor, or canonical Task UUIDv7 address")
    }
}
impl std::error::Error for MediaUsageError {}

fn task_identity(id: TaskId) -> Result<TaskId, MediaUsageError> {
    (id.as_uuid().get_version_num() == 7 && id.as_uuid().get_variant() == uuid::Variant::RFC4122)
        .then_some(id)
        .ok_or(MediaUsageError)
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
pub struct MediaUsageCursor {
    cursor: veoveo_types::OpaqueCursor<MediaUsageCursorCodec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MediaUsageCursorCodec;
impl veoveo_types::CursorCodec for MediaUsageCursorCodec {
    type Position = TaskId;
    type Error = MediaUsageError;
    fn check(&self, position: &Self::Position) -> Result<(), Self::Error> {
        task_identity(*position).map(|_| ())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 1,
            collection: (MediaUsageIndexUri::ROOT).to_owned(),
            after: *position,
        })
        .expect("closed owner cursor fields serialize");
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(MediaUsageError);
        }
        let bytes = URL_SAFE_NO_PAD.decode(wire).map_err(|_| MediaUsageError)?;
        let decoded: CursorWire = serde_json::from_slice(&bytes).map_err(|_| MediaUsageError)?;
        if decoded.version != 1 || decoded.collection != MediaUsageIndexUri::ROOT {
            return Err(MediaUsageError);
        }
        let position = decoded.after;
        Ok(position)
    }
}
impl MediaUsageCursor {
    /// Positions require native Task identities.
    /// ```compile_fail
    /// use veoveo_media_mcp::contract::MediaUsageCursor;
    /// MediaUsageCursor::new("raw-task-id");
    /// ```
    pub fn new(after: TaskId) -> Result<Self, MediaUsageError> {
        let cursor = veoveo_types::OpaqueCursor::try_new(MediaUsageCursorCodec, after)?;
        Ok(Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, MediaUsageError> {
        veoveo_types::OpaqueCursor::parse(MediaUsageCursorCodec, wire).map(|cursor| Self { cursor })
    }
    pub fn after(&self) -> TaskId {
        *self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
impl TryFrom<String> for MediaUsageCursor {
    type Error = MediaUsageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MediaUsageCursor> for String {
    fn from(value: MediaUsageCursor) -> Self {
        value.cursor.into_wire()
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="media://usage{?cursor}", error=MediaUsageError, route_error=|_| MediaUsageError, wire)]
pub struct MediaUsageIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageCursorCodec, error=|_| MediaUsageError)]
    cursor: Option<MediaUsageCursor>,
}

impl MediaUsageIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(cursor: Option<&MediaUsageCursor>) -> Self {
        Self::resource_from_parts(cursor.cloned()).expect("typed usage index address")
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaUsageError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MediaUsageError)?;
        <Self as ResourceAddress>::parse(&uri)
    }

    pub fn cursor(&self) -> Option<&MediaUsageCursor> {
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
#[resource(template="media://usage/task/{task_id}", error=MediaUsageError, route_error=|_| MediaUsageError, wire)]
pub struct MediaTaskUsageUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageTaskCodec, error=|_| MediaUsageError)]
    task_id: TaskId,
}

impl MediaTaskUsageUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    /// Database identities cannot be used as Task addresses.
    /// ```compile_fail
    /// use veoveo_media_mcp::contract::{MediaDatabaseId, MediaTaskUsageUri};
    /// MediaTaskUsageUri::new(MediaDatabaseId::new("metrics").unwrap());
    /// ```
    pub fn new(task_id: TaskId) -> Result<Self, MediaUsageError> {
        Self::resource_from_parts(task_identity(task_id)?)
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaUsageError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MediaUsageError)?;
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
impl ResourceFieldCodec<MediaUsageCursor> for UsageCursorCodec {
    type Error = MediaUsageError;
    fn parse(value: &str) -> Result<MediaUsageCursor, Self::Error> {
        MediaUsageCursor::parse(value)
    }
    fn text(value: &MediaUsageCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
struct UsageTaskCodec;
impl ResourceFieldCodec<TaskId> for UsageTaskCodec {
    type Error = MediaUsageError;
    fn parse(value: &str) -> Result<TaskId, Self::Error> {
        task_identity(value.parse().map_err(|_| MediaUsageError)?)
    }
    fn text(value: &TaskId) -> std::borrow::Cow<'_, str> {
        value.to_string().into()
    }
}
impl fmt::Display for MediaUsageIndexUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl fmt::Display for MediaTaskUsageUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EntryWire", into = "EntryWire")]
pub struct MediaUsageEntry {
    usage_uri: MediaTaskUsageUri,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    task_id: TaskId,
    usage_uri: MediaTaskUsageUri,
}

impl MediaUsageEntry {
    pub fn new(task_id: TaskId) -> Result<Self, MediaUsageError> {
        Ok(Self {
            usage_uri: MediaTaskUsageUri::new(task_id)?,
        })
    }
    pub fn task_id(&self) -> TaskId {
        self.usage_uri.task_id()
    }
    pub fn usage_uri(&self) -> &MediaTaskUsageUri {
        &self.usage_uri
    }
}
impl veoveo_types::Check for EntryWire {
    type Error = MediaUsageError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.task_id != self.usage_uri.task_id() {
            return Err(MediaUsageError);
        }

        Ok(())
    }
}
impl TryFrom<EntryWire> for MediaUsageEntry {
    type Error = MediaUsageError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
        Ok(Self {
            usage_uri: value.usage_uri,
        })
    }
}
impl From<MediaUsageEntry> for EntryWire {
    fn from(value: MediaUsageEntry) -> Self {
        Self {
            task_id: value.task_id(),
            usage_uri: value.usage_uri,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct MediaUsagePage {
    items: Vec<MediaUsageEntry>,
    next_cursor: Option<MediaUsageCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    items: Vec<MediaUsageEntry>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    next_cursor: Option<MediaUsageCursor>,
}

impl MediaUsagePage {
    pub fn from_task_ids(ids: Vec<TaskId>, next: Option<TaskId>) -> Result<Self, MediaUsageError> {
        Self::try_from(PageWire {
            items: ids
                .into_iter()
                .map(MediaUsageEntry::new)
                .collect::<Result<_, _>>()?,
            limit: MEDIA_USAGE_PAGE_SIZE,
            next_cursor: next.map(MediaUsageCursor::new).transpose()?,
        })
    }

    pub fn items(&self) -> &[MediaUsageEntry] {
        &self.items
    }
    pub fn next_cursor(&self) -> Option<&MediaUsageCursor> {
        self.next_cursor.as_ref()
    }
}
impl veoveo_types::Check for PageWire {
    type Error = MediaUsageError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != MEDIA_USAGE_PAGE_SIZE
            || self.items.len() > MEDIA_USAGE_PAGE_SIZE
            || self
                .items
                .windows(2)
                .any(|pair| pair[0].task_id() >= pair[1].task_id())
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.items.len() != MEDIA_USAGE_PAGE_SIZE
                    || self.items.last().map(MediaUsageEntry::task_id) != Some(cursor.after())
            })
        {
            return Err(MediaUsageError);
        }

        Ok(())
    }
}
impl TryFrom<PageWire> for MediaUsagePage {
    type Error = MediaUsageError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
        Ok(Self {
            items: value.items,
            next_cursor: value.next_cursor,
        })
    }
}
impl From<MediaUsagePage> for PageWire {
    fn from(value: MediaUsagePage) -> Self {
        Self {
            items: value.items,
            limit: MEDIA_USAGE_PAGE_SIZE,
            next_cursor: value.next_cursor,
        }
    }
}
