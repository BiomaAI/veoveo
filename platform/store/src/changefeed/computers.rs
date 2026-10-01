//! Typed identities for Computer state and grant invalidations.
use crate::{ChangefeedEntry, StoreError, TaskChange};
use surrealdb::types::{RecordIdKey, Value};
use veoveo_computers_contract::{AutomationGrantId, ComputerId};
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
