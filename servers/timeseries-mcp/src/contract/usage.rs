//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceFieldCodec, ResourceUri, TaskId};

pub const TIMESERIES_USAGE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("expected a valid Timeseries usage page, cursor, or canonical Task UUIDv7 address")]
pub struct TimeseriesUsageError;

fn task_identity(id: TaskId) -> Result<TaskId, TimeseriesUsageError> {
    (id.as_uuid().get_version_num() == 7 && id.as_uuid().get_variant() == uuid::Variant::RFC4122)
        .then_some(id)
        .ok_or(TimeseriesUsageError)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct CursorWire {
    version: u8,
    task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TimeseriesUsageCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<TimeseriesUsageCursorCodec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TimeseriesUsageCursorCodec;
impl veoveo_types::StatelessCursorCodec for TimeseriesUsageCursorCodec {}
impl veoveo_types::CursorCodec for TimeseriesUsageCursorCodec {
    type Position = TaskId;
    type Error = TimeseriesUsageError;
    fn check(&self, position: &Self::Position) -> Result<(), Self::Error> {
        task_identity(*position).map(|_| ())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 2,
            task_id: *position,
        })
        .expect("closed owner cursor fields serialize");
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(TimeseriesUsageError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| TimeseriesUsageError)?;
        let decoded: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| TimeseriesUsageError)?;
        if decoded.version != 2 {
            return Err(TimeseriesUsageError);
        }
        let position = decoded.task_id;
        Ok(position)
    }
}
impl TimeseriesUsageCursor {
    /// Positions require native Task identities.
    /// ```compile_fail
    /// use veoveo_timeseries_mcp::contract::TimeseriesUsageCursor;
    /// TimeseriesUsageCursor::new("raw-task-id");
    /// ```
    pub fn new(after: TaskId) -> Result<Self, TimeseriesUsageError> {
        let cursor = veoveo_types::OpaqueCursor::try_new(TimeseriesUsageCursorCodec, after)?;
        Ok(Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, TimeseriesUsageError> {
        veoveo_types::OpaqueCursor::parse(TimeseriesUsageCursorCodec, wire)
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
    cached(TimeseriesUsageErrorAddresses),
    template = "timeseries://usage{?cursor}"
)]
pub struct TimeseriesUsageIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageCursorCodec, error=|_| TimeseriesUsageError, accessor = cursor, argument = optional_borrowed)]
    cursor: Option<TimeseriesUsageCursor>,
}

impl TimeseriesUsageIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
}

#[veoveo_types::resource_address(cached_checked(TimeseriesUsageErrorAddresses), template = "timeseries://usage/task/{task_id}", traits = cloned, schema = derived)]
pub struct TimeseriesTaskUsageUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageTaskCodec, error=|_| TimeseriesUsageError, accessor = task_id, copy_accessor, admit = task_identity)]
    task_id: TaskId,
}

impl TimeseriesTaskUsageUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
}

struct UsageCursorCodec;
impl ResourceFieldCodec<TimeseriesUsageCursor> for UsageCursorCodec {
    type Error = TimeseriesUsageError;
    fn parse(value: &str) -> Result<TimeseriesUsageCursor, Self::Error> {
        TimeseriesUsageCursor::parse(value)
    }
    fn text(value: &TimeseriesUsageCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
struct UsageTaskCodec;
impl ResourceFieldCodec<TaskId> for UsageTaskCodec {
    type Error = TimeseriesUsageError;
    fn parse(value: &str) -> Result<TaskId, Self::Error> {
        task_identity(value.parse().map_err(|_| TimeseriesUsageError)?)
    }
    fn text(value: &TaskId) -> std::borrow::Cow<'_, str> {
        value.to_string().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EntryWire", into = "EntryWire")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeseriesUsageEntry {
    usage_uri: TimeseriesTaskUsageUri,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
impl veoveo_types::Check for EntryWire {
    type Error = TimeseriesUsageError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.task_id != self.usage_uri.task_id() {
            return Err(TimeseriesUsageError);
        }

        Ok(())
    }
}
impl TryFrom<EntryWire> for TimeseriesUsageEntry {
    type Error = TimeseriesUsageError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
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
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeseriesUsagePage {
    usage: Vec<TimeseriesUsageEntry>,
    next_cursor: Option<TimeseriesUsageCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
impl veoveo_types::Check for PageWire {
    type Error = TimeseriesUsageError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != TIMESERIES_USAGE_PAGE_SIZE
            || self.usage.len() > TIMESERIES_USAGE_PAGE_SIZE
            || self
                .usage
                .windows(2)
                .any(|pair| pair[0].task_id() >= pair[1].task_id())
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.usage.len() != TIMESERIES_USAGE_PAGE_SIZE
                    || self.usage.last().map(TimeseriesUsageEntry::task_id) != Some(cursor.after())
            })
        {
            return Err(TimeseriesUsageError);
        }

        Ok(())
    }
}
impl TryFrom<PageWire> for TimeseriesUsagePage {
    type Error = TimeseriesUsageError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
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

#[doc(hidden)]
pub struct TimeseriesUsageErrorAddresses;
impl veoveo_types::ResourceProfile for TimeseriesUsageErrorAddresses {
    type Error = TimeseriesUsageError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| TimeseriesUsageError,
        };
}
