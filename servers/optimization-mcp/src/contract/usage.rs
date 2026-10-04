//! Caller-owned usage pages and addresses, independent of Store and TaskRuntime.
use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri, TaskId};

pub const OPTIMIZATION_USAGE_PAGE_SIZE: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("expected a valid Optimization usage page, cursor, or canonical Task UUIDv7 address")]
pub struct OptimizationUsageError;

fn task_identity(id: TaskId) -> Result<TaskId, OptimizationUsageError> {
    (id.as_uuid().get_version_num() == 7 && id.as_uuid().get_variant() == uuid::Variant::RFC4122)
        .then_some(id)
        .ok_or(OptimizationUsageError)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct OptimizationUsageCursor {
    cursor: veoveo_types::OpaqueCursor<OptimizationUsageCursorCodec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OptimizationUsageCursorCodec;
impl veoveo_types::CursorCodec for OptimizationUsageCursorCodec {
    type Position = TaskId;
    type Error = OptimizationUsageError;
    fn check(&self, position: &Self::Position) -> Result<(), Self::Error> {
        task_identity(*position).map(|_| ())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 1,
            task_id: *position,
        })
        .expect("closed owner cursor fields serialize");
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(OptimizationUsageError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| OptimizationUsageError)?;
        let decoded: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| OptimizationUsageError)?;
        if decoded.version != 1 {
            return Err(OptimizationUsageError);
        }
        let position = decoded.task_id;
        Ok(position)
    }
}
impl OptimizationUsageCursor {
    /// Positions require native Task identities.
    /// ```compile_fail
    /// use veoveo_optimization_mcp::contract::OptimizationUsageCursor;
    /// OptimizationUsageCursor::new("raw-task-id");
    /// ```
    pub fn new(after: TaskId) -> Result<Self, OptimizationUsageError> {
        let cursor = veoveo_types::OpaqueCursor::try_new(OptimizationUsageCursorCodec, after)?;
        Ok(Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, OptimizationUsageError> {
        veoveo_types::OpaqueCursor::parse(OptimizationUsageCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }
    pub fn after(&self) -> TaskId {
        *self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
impl TryFrom<String> for OptimizationUsageCursor {
    type Error = OptimizationUsageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<OptimizationUsageCursor> for String {
    fn from(value: OptimizationUsageCursor) -> Self {
        value.cursor.into_wire()
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="optimization://usage{?cursor}", error=OptimizationUsageError, route_error=|_| OptimizationUsageError, wire)]
pub struct OptimizationUsageIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageCursorCodec, error=|_| OptimizationUsageError)]
    cursor: Option<OptimizationUsageCursor>,
}

impl OptimizationUsageIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(cursor: Option<&OptimizationUsageCursor>) -> Self {
        Self::resource_from_parts(cursor.cloned()).expect("typed usage index address")
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationUsageError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| OptimizationUsageError)?;
        <Self as ResourceAddress>::parse(&uri)
    }

    pub fn cursor(&self) -> Option<&OptimizationUsageCursor> {
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
#[resource(template="optimization://usage/task/{task_id}", error=OptimizationUsageError, route_error=|_| OptimizationUsageError, wire)]
pub struct OptimizationTaskUsageUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=UsageTaskCodec, error=|_| OptimizationUsageError)]
    task_id: TaskId,
}

impl OptimizationTaskUsageUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(task_id: TaskId) -> Result<Self, OptimizationUsageError> {
        Self::resource_from_parts(task_identity(task_id)?)
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationUsageError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| OptimizationUsageError)?;
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
impl ResourceFieldCodec<OptimizationUsageCursor> for UsageCursorCodec {
    type Error = OptimizationUsageError;
    fn parse(value: &str) -> Result<OptimizationUsageCursor, Self::Error> {
        OptimizationUsageCursor::parse(value)
    }
    fn text(value: &OptimizationUsageCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
struct UsageTaskCodec;
impl ResourceFieldCodec<TaskId> for UsageTaskCodec {
    type Error = OptimizationUsageError;
    fn parse(value: &str) -> Result<TaskId, Self::Error> {
        task_identity(value.parse().map_err(|_| OptimizationUsageError)?)
    }
    fn text(value: &TaskId) -> std::borrow::Cow<'_, str> {
        value.to_string().into()
    }
}
impl fmt::Display for OptimizationUsageIndexUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl fmt::Display for OptimizationTaskUsageUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EntryWire", into = "EntryWire")]
pub struct OptimizationUsageEntry {
    usage_uri: OptimizationTaskUsageUri,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    task_id: TaskId,
    usage_uri: OptimizationTaskUsageUri,
}

impl OptimizationUsageEntry {
    pub fn new(task_id: TaskId) -> Result<Self, OptimizationUsageError> {
        Ok(Self {
            usage_uri: OptimizationTaskUsageUri::new(task_id)?,
        })
    }
    pub fn task_id(&self) -> TaskId {
        self.usage_uri.task_id()
    }
    pub fn usage_uri(&self) -> &OptimizationTaskUsageUri {
        &self.usage_uri
    }
}
impl veoveo_types::Check for EntryWire {
    type Error = OptimizationUsageError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.task_id != self.usage_uri.task_id() {
            return Err(OptimizationUsageError);
        }

        Ok(())
    }
}
impl TryFrom<EntryWire> for OptimizationUsageEntry {
    type Error = OptimizationUsageError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
        Ok(Self {
            usage_uri: value.usage_uri,
        })
    }
}
impl From<OptimizationUsageEntry> for EntryWire {
    fn from(value: OptimizationUsageEntry) -> Self {
        Self {
            task_id: value.task_id(),
            usage_uri: value.usage_uri,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct OptimizationUsagePage {
    usage: Vec<OptimizationUsageEntry>,
    next_cursor: Option<OptimizationUsageCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    usage: Vec<OptimizationUsageEntry>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_cursor: Option<OptimizationUsageCursor>,
}

impl OptimizationUsagePage {
    pub fn from_task_ids(
        ids: Vec<TaskId>,
        next: Option<TaskId>,
    ) -> Result<Self, OptimizationUsageError> {
        Self::try_from(PageWire {
            usage: ids
                .into_iter()
                .map(OptimizationUsageEntry::new)
                .collect::<Result<_, _>>()?,
            limit: OPTIMIZATION_USAGE_PAGE_SIZE,
            next_cursor: next.map(OptimizationUsageCursor::new).transpose()?,
        })
    }

    pub fn usage(&self) -> &[OptimizationUsageEntry] {
        &self.usage
    }
    pub fn next_cursor(&self) -> Option<&OptimizationUsageCursor> {
        self.next_cursor.as_ref()
    }
}
impl veoveo_types::Check for PageWire {
    type Error = OptimizationUsageError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != OPTIMIZATION_USAGE_PAGE_SIZE
            || self.usage.len() > OPTIMIZATION_USAGE_PAGE_SIZE
            || self
                .usage
                .windows(2)
                .any(|pair| pair[0].task_id() >= pair[1].task_id())
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.usage.len() != OPTIMIZATION_USAGE_PAGE_SIZE
                    || self.usage.last().map(OptimizationUsageEntry::task_id)
                        != Some(cursor.after())
            })
        {
            return Err(OptimizationUsageError);
        }

        Ok(())
    }
}
impl TryFrom<PageWire> for OptimizationUsagePage {
    type Error = OptimizationUsageError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
        Ok(Self {
            usage: value.usage,
            next_cursor: value.next_cursor,
        })
    }
}
impl From<OptimizationUsagePage> for PageWire {
    fn from(value: OptimizationUsagePage) -> Self {
        Self {
            usage: value.usage,
            limit: OPTIMIZATION_USAGE_PAGE_SIZE,
            next_cursor: value.next_cursor,
        }
    }
}
