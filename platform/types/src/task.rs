use schemars::{Schema, SchemaGenerator};

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
/// observe_task(WorkContextId::parse("operations").unwrap());
/// ```
#[veoveo_types::id(uuid(TaskIds), fresh)]
pub struct TaskId(Uuid);

impl TaskId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}

#[doc(hidden)]
pub struct TaskIds;
impl crate::IdProfile for TaskIds {
    type Error = uuid::Error;
    const PROFILE: crate::IdProfileSpec<Self::Error> = crate::IdProfileSpec {
        wire: crate::IdWire::InnerUuid,
        schema_id: Some(|_| concat!(module_path!(), "::TaskId").into()),
        ..crate::IdProfileSpec::generated_uuid(
            crate::UuidGrammar {
                versions: &[],
                variant: crate::UuidVariant::Any,
                spelling: crate::UuidSpelling::ParserAliases,
            },
            task_id_error,
        )
    }
    .owner_schema(task_schema, false);
}
fn task_id_error(value: &str, _: crate::IdMetadata, _: crate::IdFailure) -> uuid::Error {
    Uuid::parse_str(value)
        .expect_err("only malformed UUIDs reach the unrestricted Task error mapping")
}
fn task_schema(_: &mut SchemaGenerator, _: crate::IdMetadata) -> Schema {
    // UUID's parser also admits simple, braced, and lowercase-prefix URN
    // spellings. A `format: uuid` schema would narrow that input profile.
    let hyphenated = "[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}";
    schemars::json_schema!({
        "type": "string",
        "pattern": format!("^(?:[0-9A-Fa-f]{{32}}|{hyphenated}|\\{{{hyphenated}\\}}|urn:uuid:{hyphenated})(?![\\s\\S])"),
        "maxLength": 45
    })
}
