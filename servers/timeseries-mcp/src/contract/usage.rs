//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.
use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, TaskId, UriSegment,
};

pub const TIMESERIES_USAGE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeseriesUsageError;

impl fmt::Display for TimeseriesUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "expected a valid Timeseries usage page, cursor, or canonical Task UUIDv7 address",
        )
    }
}
impl std::error::Error for TimeseriesUsageError {}

fn task_identity(id: TaskId) -> Result<TaskId, TimeseriesUsageError> {
    (id.as_uuid().get_version_num() == 7 && id.as_uuid().get_variant() == uuid::Variant::RFC4122)
        .then_some(id)
        .ok_or(TimeseriesUsageError)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct TimeseriesUsageCursor {
    wire: String,
    after: TaskId,
}

impl TimeseriesUsageCursor {
    /// Positions require native Task identities.
    /// ```compile_fail
    /// use veoveo_timeseries_mcp::contract::TimeseriesUsageCursor;
    /// TimeseriesUsageCursor::new("raw-task-id");
    /// ```
    pub fn new(after: TaskId) -> Result<Self, TimeseriesUsageError> {
        let after = task_identity(after)?;
        let wire = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&CursorWire {
                version: 1,
                task_id: after,
            })
            .expect("closed usage cursor fields serialize"),
        );
        Ok(Self { wire, after })
    }

    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeseriesUsageError> {
        let wire = wire.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(TimeseriesUsageError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(&wire)
            .map_err(|_| TimeseriesUsageError)?;
        let cursor: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| TimeseriesUsageError)?;
        if cursor.version != 1 {
            return Err(TimeseriesUsageError);
        }
        Ok(Self {
            wire,
            after: task_identity(cursor.task_id)?,
        })
    }

    pub fn after(&self) -> TaskId {
        self.after
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}

impl TryFrom<String> for TimeseriesUsageCursor {
    type Error = TimeseriesUsageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<TimeseriesUsageCursor> for String {
    fn from(value: TimeseriesUsageCursor) -> Self {
        value.wire
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct TimeseriesUsageIndexUri {
    wire: ResourceUri,
    cursor: Option<TimeseriesUsageCursor>,
}

impl TimeseriesUsageIndexUri {
    pub const ROOT: &str = "timeseries://usage";
    pub const TEMPLATE: &str = "timeseries://usage{?cursor}";

    pub fn new(cursor: Option<&TimeseriesUsageCursor>) -> Self {
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

    pub fn parse(value: impl AsRef<str>) -> Result<Self, TimeseriesUsageError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| TimeseriesUsageError)?;
        if parts.scheme() != "timeseries"
            || parts.authority() != "usage"
            || parts.path_segments().next().is_some()
        {
            return Err(TimeseriesUsageError);
        }
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(TimeseriesUsageError);
            }
            Some(TimeseriesUsageCursor::parse(
                query.get("cursor").ok_or(TimeseriesUsageError)?.clone(),
            )?)
        } else {
            None
        };
        let uri = Self::new(cursor.as_ref());
        if uri.as_str() != value {
            return Err(TimeseriesUsageError);
        }
        Ok(uri)
    }

    pub fn cursor(&self) -> Option<&TimeseriesUsageCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct TimeseriesTaskUsageUri {
    wire: ResourceUri,
    task_id: TaskId,
}

impl TimeseriesTaskUsageUri {
    pub const TEMPLATE: &str = "timeseries://usage/task/{task_id}";

    pub fn new(task_id: TaskId) -> Result<Self, TimeseriesUsageError> {
        let task_id = task_identity(task_id)?;
        let wire = ResourceUriBuilder::new(TimeseriesUsageIndexUri::ROOT)
            .expect("declared usage root")
            .segment(UriSegment::new("task").expect("declared task segment"))
            .segment(UriSegment::new(task_id.to_string()).expect("canonical native Task UUID"))
            .build()
            .expect("typed Task usage URI");
        Ok(Self { wire, task_id })
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, TimeseriesUsageError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| TimeseriesUsageError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        if parts.scheme() != "timeseries"
            || parts.authority() != "usage"
            || parts.has_query()
            || path.len() != 2
            || path[0] != "task"
        {
            return Err(TimeseriesUsageError);
        }
        let uri = Self::new(path[1].parse().map_err(|_| TimeseriesUsageError)?)?;
        if uri.as_str() != value {
            return Err(TimeseriesUsageError);
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
            type Error = TimeseriesUsageError;
            fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(uri.as_str())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok(self.wire.clone())
            }
        }
        impl TryFrom<String> for $name {
            type Error = TimeseriesUsageError;
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
address_traits!(TimeseriesUsageIndexUri);
address_traits!(TimeseriesTaskUsageUri);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EntryWire", into = "EntryWire")]
pub struct TimeseriesUsageEntry {
    usage_uri: TimeseriesTaskUsageUri,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    task_id: TaskId,
    usage_uri: TimeseriesTaskUsageUri,
}

impl TimeseriesUsageEntry {
    pub fn new(task_id: TaskId) -> Result<Self, TimeseriesUsageError> {
        Ok(Self {
            usage_uri: TimeseriesTaskUsageUri::new(task_id)?,
        })
    }
    pub fn task_id(&self) -> TaskId {
        self.usage_uri.task_id()
    }
    pub fn usage_uri(&self) -> &TimeseriesTaskUsageUri {
        &self.usage_uri
    }
}
impl TryFrom<EntryWire> for TimeseriesUsageEntry {
    type Error = TimeseriesUsageError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        if value.task_id != value.usage_uri.task_id() {
            return Err(TimeseriesUsageError);
        }
        Ok(Self {
            usage_uri: value.usage_uri,
        })
    }
}
impl From<TimeseriesUsageEntry> for EntryWire {
    fn from(value: TimeseriesUsageEntry) -> Self {
        Self {
            task_id: value.task_id(),
            usage_uri: value.usage_uri,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct TimeseriesUsagePage {
    usage: Vec<TimeseriesUsageEntry>,
    next_cursor: Option<TimeseriesUsageCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    usage: Vec<TimeseriesUsageEntry>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_cursor: Option<TimeseriesUsageCursor>,
}

impl TimeseriesUsagePage {
    pub fn from_task_ids(
        ids: Vec<TaskId>,
        next: Option<TaskId>,
    ) -> Result<Self, TimeseriesUsageError> {
        Self::try_from(PageWire {
            usage: ids
                .into_iter()
                .map(TimeseriesUsageEntry::new)
                .collect::<Result<_, _>>()?,
            limit: TIMESERIES_USAGE_PAGE_SIZE,
            next_cursor: next.map(TimeseriesUsageCursor::new).transpose()?,
        })
    }

    pub fn usage(&self) -> &[TimeseriesUsageEntry] {
        &self.usage
    }
    pub fn next_cursor(&self) -> Option<&TimeseriesUsageCursor> {
        self.next_cursor.as_ref()
    }
}
impl TryFrom<PageWire> for TimeseriesUsagePage {
    type Error = TimeseriesUsageError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        if value.limit != TIMESERIES_USAGE_PAGE_SIZE
            || value.usage.len() > TIMESERIES_USAGE_PAGE_SIZE
            || value
                .usage
                .windows(2)
                .any(|pair| pair[0].task_id() >= pair[1].task_id())
            || value.next_cursor.as_ref().is_some_and(|cursor| {
                value.usage.len() != TIMESERIES_USAGE_PAGE_SIZE
                    || value.usage.last().map(TimeseriesUsageEntry::task_id) != Some(cursor.after())
            })
        {
            return Err(TimeseriesUsageError);
        }
        Ok(Self {
            usage: value.usage,
            next_cursor: value.next_cursor,
        })
    }
}
impl From<TimeseriesUsagePage> for PageWire {
    fn from(value: TimeseriesUsagePage) -> Self {
        Self {
            usage: value.usage,
            limit: TIMESERIES_USAGE_PAGE_SIZE,
            next_cursor: value.next_cursor,
        }
    }
}
