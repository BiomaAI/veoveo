//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.
use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, TaskId, UriSegment,
};

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
    wire: String,
    after: TaskId,
}

impl MediaUsageCursor {
    /// Positions require native Task identities.
    /// ```compile_fail
    /// use veoveo_media_mcp::contract::MediaUsageCursor;
    /// MediaUsageCursor::new("raw-task-id");
    /// ```
    pub fn new(after: TaskId) -> Result<Self, MediaUsageError> {
        let after = task_identity(after)?;
        let wire = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&CursorWire {
                version: 1,
                collection: MediaUsageIndexUri::ROOT.to_owned(),
                after,
            })
            .expect("closed usage cursor fields serialize"),
        );
        Ok(Self { wire, after })
    }

    pub fn parse(wire: impl Into<String>) -> Result<Self, MediaUsageError> {
        let wire = wire.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(MediaUsageError);
        }
        let bytes = URL_SAFE_NO_PAD.decode(&wire).map_err(|_| MediaUsageError)?;
        let cursor: CursorWire = serde_json::from_slice(&bytes).map_err(|_| MediaUsageError)?;
        if cursor.version != 1 || cursor.collection != MediaUsageIndexUri::ROOT {
            return Err(MediaUsageError);
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

impl TryFrom<String> for MediaUsageCursor {
    type Error = MediaUsageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MediaUsageCursor> for String {
    fn from(value: MediaUsageCursor) -> Self {
        value.wire
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MediaUsageIndexUri {
    wire: ResourceUri,
    cursor: Option<MediaUsageCursor>,
}

impl MediaUsageIndexUri {
    pub const ROOT: &str = "media://usage";
    pub const TEMPLATE: &str = "media://usage{?cursor}";

    pub fn new(cursor: Option<&MediaUsageCursor>) -> Self {
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

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaUsageError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MediaUsageError)?;
        if parts.scheme() != "media"
            || parts.authority() != "usage"
            || parts.path_segments().next().is_some()
        {
            return Err(MediaUsageError);
        }
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(MediaUsageError);
            }
            Some(MediaUsageCursor::parse(
                query.get("cursor").ok_or(MediaUsageError)?.clone(),
            )?)
        } else {
            None
        };
        let uri = Self::new(cursor.as_ref());
        if uri.as_str() != value {
            return Err(MediaUsageError);
        }
        Ok(uri)
    }

    pub fn cursor(&self) -> Option<&MediaUsageCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MediaTaskUsageUri {
    wire: ResourceUri,
    task_id: TaskId,
}

impl MediaTaskUsageUri {
    pub const TEMPLATE: &str = "media://usage/task/{task_id}";

    /// Database identities cannot be used as Task addresses.
    /// ```compile_fail
    /// use veoveo_media_mcp::contract::{MediaDatabaseId, MediaTaskUsageUri};
    /// MediaTaskUsageUri::new(MediaDatabaseId::new("metrics").unwrap());
    /// ```
    pub fn new(task_id: TaskId) -> Result<Self, MediaUsageError> {
        let task_id = task_identity(task_id)?;
        let wire = ResourceUriBuilder::new(MediaUsageIndexUri::ROOT)
            .expect("declared usage root")
            .segment(UriSegment::new("task").expect("declared task segment"))
            .segment(UriSegment::new(task_id.to_string()).expect("canonical native Task UUID"))
            .build()
            .expect("typed Task usage URI");
        Ok(Self { wire, task_id })
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaUsageError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MediaUsageError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        if parts.scheme() != "media"
            || parts.authority() != "usage"
            || parts.has_query()
            || path.len() != 2
            || path[0] != "task"
        {
            return Err(MediaUsageError);
        }
        let uri = Self::new(path[1].parse().map_err(|_| MediaUsageError)?)?;
        if uri.as_str() != value {
            return Err(MediaUsageError);
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
            type Error = MediaUsageError;
            fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(uri.as_str())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok(self.wire.clone())
            }
        }
        impl TryFrom<String> for $name {
            type Error = MediaUsageError;
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
address_traits!(MediaUsageIndexUri);
address_traits!(MediaTaskUsageUri);

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
impl TryFrom<EntryWire> for MediaUsageEntry {
    type Error = MediaUsageError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        if value.task_id != value.usage_uri.task_id() {
            return Err(MediaUsageError);
        }
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
impl TryFrom<PageWire> for MediaUsagePage {
    type Error = MediaUsageError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        if value.limit != MEDIA_USAGE_PAGE_SIZE
            || value.items.len() > MEDIA_USAGE_PAGE_SIZE
            || value
                .items
                .windows(2)
                .any(|pair| pair[0].task_id() >= pair[1].task_id())
            || value.next_cursor.as_ref().is_some_and(|cursor| {
                value.items.len() != MEDIA_USAGE_PAGE_SIZE
                    || value.items.last().map(MediaUsageEntry::task_id) != Some(cursor.after())
            })
        {
            return Err(MediaUsageError);
        }
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
