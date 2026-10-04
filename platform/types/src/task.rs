use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identity of a native Veoveo Task, independent of its storage or MCP projection.
///
/// Generation uses UUIDv7. Parsing preserves the existing UUID admission profile;
/// the Task runtime additionally requires v7 at its external lookup boundary.
/// This identity establishes neither Task existence nor authority.
///
/// ```compile_fail
/// use veoveo_types::{TaskId, WorkContextId};
/// fn observe_task(_: TaskId) {}
/// observe_task(WorkContextId::new("operations").unwrap());
/// ```
#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
)]
#[serde(transparent)]
#[id(error = uuid::Error, admit = Uuid::parse_str)]
pub struct TaskId(Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<TaskId> for Uuid {
    fn from(value: TaskId) -> Self {
        value.0
    }
}

impl JsonSchema for TaskId {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("TaskId")
    }

    fn schema_id() -> Cow<'static, str> {
        Cow::Borrowed(concat!(module_path!(), "::TaskId"))
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        // UUID's parser also admits simple, braced, and lowercase-prefix URN
        // spellings. A `format: uuid` schema would narrow that input profile.
        let hyphenated =
            "[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}";
        schemars::json_schema!({
            "type": "string",
            "pattern": format!("^(?:[0-9A-Fa-f]{{32}}|{hyphenated}|\\{{{hyphenated}\\}}|urn:uuid:{hyphenated})(?![\\s\\S])"),
            "maxLength": 45
        })
    }
}
