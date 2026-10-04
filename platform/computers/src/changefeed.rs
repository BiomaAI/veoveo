//! Typed identities for Computer state and grant invalidations.
use surrealdb::types::{RecordIdKey, Value};
use veoveo_computers_contract::{AutomationGrantId, ComputerId};
use veoveo_platform_store::{ChangefeedEntry, StoreError, TaskChange};
use veoveo_types::TaskId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComputerChange {
    Computer(ComputerId),
    Automation {
        computer: ComputerId,
        grant: AutomationGrantId,
    },
    Task(TaskId),
}
impl ComputerChange {
    pub fn decode(entry: &ChangefeedEntry) -> Result<Option<Self>, StoreError> {
        if let Some(change) = TaskChange::decode(entry)? {
            return Ok(Some(Self::Task(change.task_id)));
        }
        let Some(table) = entry.table() else {
            return Ok(None);
        };
        if !matches!(
            table,
            "computer"
                | "computer_automation_grant"
                | "computer_session_grant"
                | "computer_cli_grant"
                | "computer_maintenance"
        ) {
            return Ok(None);
        }
        let invalid = || StoreError::InvalidChangefeedEntry {
            reason: "Computer change requires typed computer and grant identities",
        };
        let row = match entry {
            ChangefeedEntry::Upsert(row) => Some(row),
            ChangefeedEntry::Delete { original, .. } => original.as_ref(),
            ChangefeedEntry::Definition => None,
        };
        let id = match row.map(|row| row.get("computer_id")) {
            Some(Value::Uuid(id)) => **id,
            _ if table == "computer" => match entry.record_id().map(|id| &id.key) {
                Some(RecordIdKey::Uuid(id)) => **id,
                _ => return Err(invalid()),
            },
            _ => return Err(invalid()),
        };
        let computer = ComputerId::try_from(id).map_err(|_| invalid())?;
        if table == "computer_automation_grant" {
            let grant = match row.map(|row| row.get("grant_id")) {
                Some(Value::Uuid(id)) => {
                    AutomationGrantId::try_from(**id).map_err(|_| invalid())?
                }
                _ => return Err(invalid()),
            };
            Ok(Some(Self::Automation { computer, grant }))
        } else {
            Ok(Some(Self::Computer(computer)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use surrealdb::types::{Object, RecordId, Uuid as DbUuid};

    fn grant_row(computer: Value, grant: Value) -> Value {
        let mut row = Object::default();
        row.insert(
            "id",
            RecordId::new(
                "computer_automation_grant",
                DbUuid::from(uuid::Uuid::now_v7()),
            ),
        );
        row.insert("computer_id", computer);
        row.insert("grant_id", grant);
        Value::Object(row)
    }

    #[test]
    fn automation_rows_and_deleted_originals_keep_typed_parent_and_grant() {
        let computer = ComputerId::new();
        let grant = AutomationGrantId::new();
        let row = grant_row(
            Value::Uuid(DbUuid::from(computer.into_uuid())),
            Value::Uuid(DbUuid::from(grant.into_uuid())),
        );
        let expected = Some(ComputerChange::Automation { computer, grant });
        assert_eq!(
            ComputerChange::decode(&ChangefeedEntry::Upsert(row.clone())).unwrap(),
            expected
        );
        let record = match row.get("id") {
            Value::RecordId(record) => record.clone(),
            _ => unreachable!("typed fixture record"),
        };
        assert_eq!(
            ComputerChange::decode(&ChangefeedEntry::Delete {
                record: record.clone(),
                original: Some(row)
            })
            .unwrap(),
            expected
        );
        assert!(
            ComputerChange::decode(&ChangefeedEntry::Delete {
                record,
                original: None
            })
            .is_err()
        );
        for (parent, identity) in [
            (
                Value::String(computer.into_uuid().to_string()),
                Value::Uuid(DbUuid::from(grant.into_uuid())),
            ),
            (
                Value::Uuid(DbUuid::from(uuid::Uuid::new_v4())),
                Value::Uuid(DbUuid::from(grant.into_uuid())),
            ),
            (
                Value::Uuid(DbUuid::from(computer.into_uuid())),
                Value::String(grant.into_uuid().to_string()),
            ),
            (
                Value::Uuid(DbUuid::from(computer.into_uuid())),
                Value::Uuid(DbUuid::from(uuid::Uuid::new_v4())),
            ),
            (Value::None, Value::Uuid(DbUuid::from(grant.into_uuid()))),
        ] {
            assert!(
                ComputerChange::decode(&ChangefeedEntry::Upsert(grant_row(parent, identity)))
                    .is_err()
            );
        }
    }

    #[test]
    fn record_only_computer_deletions_and_shared_task_decoder_preserve_identities() {
        let computer = ComputerId::new();
        assert_eq!(
            ComputerChange::decode(&ChangefeedEntry::Delete {
                record: RecordId::new("computer", DbUuid::from(computer.into_uuid())),
                original: None,
            })
            .unwrap(),
            Some(ComputerChange::Computer(computer))
        );
        for record in [
            RecordId::new("computer", computer.into_uuid().to_string()),
            RecordId::new("computer", DbUuid::from(uuid::Uuid::new_v4())),
        ] {
            assert!(
                ComputerChange::decode(&ChangefeedEntry::Delete {
                    record,
                    original: None
                })
                .is_err()
            );
        }
        let task = TaskId::new();
        assert_eq!(
            ComputerChange::decode(&ChangefeedEntry::Delete {
                record: veoveo_platform_store::task_record_id(task),
                original: None,
            })
            .unwrap(),
            Some(ComputerChange::Task(task))
        );
        assert_eq!(
            ComputerChange::decode(&ChangefeedEntry::Definition).unwrap(),
            None
        );
        assert_eq!(
            ComputerChange::decode(&ChangefeedEntry::Delete {
                record: RecordId::new("unrelated", "external"),
                original: None,
            })
            .unwrap(),
            None
        );
    }
}
