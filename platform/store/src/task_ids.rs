use surrealdb::types::{RecordId, Uuid as SurrealUuid};
use veoveo_types::TaskId;

/// Bind a native Task identity to the Store's UUID-keyed Task table.
pub fn task_record_id(id: TaskId) -> RecordId {
    RecordId::new("task", SurrealUuid::from(id.as_uuid()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use surrealdb::types::RecordIdKey;
    use uuid::Uuid;

    #[test]
    fn task_binding_preserves_table_and_uuid_key_for_existing_values() {
        for value in [Uuid::now_v7(), Uuid::nil(), Uuid::max()] {
            let record = task_record_id(TaskId::from_uuid(value));
            assert_eq!(record.table.as_str(), "task");
            assert_eq!(record.key, RecordIdKey::Uuid(value.into()));
        }
    }
}
