//! Resolve participant-scoped policy inputs before decoding lifecycle state.
use crate::{
    ComputerError, ComputersStore, Operation, Result,
    api::{Action, AutomationGrantId, ComputerId},
};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_task_runtime::TaskOwner;

#[derive(Clone, Copy)]
pub(crate) enum OperationParticipant {
    Owner,
    Actor,
}
impl OperationParticipant {
    fn is_owner(self) -> bool {
        matches!(self, Self::Owner)
    }
}

pub(crate) struct OperationLookup {
    id: veoveo_types::TaskId,
    pub computer: ComputerId,
    pub action: Action,
    pub grant: Option<AutomationGrantId>,
    participant: OperationParticipant,
}
#[derive(SurrealValue)]
struct LookupRecord {
    computer_id: Uuid,
    action: String,
    automation_grant_id: Option<Uuid>,
}

impl ComputersStore {
    pub(crate) async fn operation_lookup(
        &self,
        caller: &TaskOwner,
        id: veoveo_types::TaskId,
        participant: OperationParticipant,
    ) -> Result<OperationLookup> {
        let mut params = crate::store::owner_query_bindings(caller)?;
        params.extend([
            (
                "operation",
                crate::operation_admission::operation_record(id).into_value(),
            ),
            (
                "provider",
                self.provider_instance_id.into_uuid().into_value(),
            ),
            ("as_owner", participant.is_owner().into_value()),
        ]);
        let mut response = self
            .query(include_str!("../queries/operation_lookup.surql"), params)
            .await?;
        let row: Option<LookupRecord> = response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let row = row.ok_or(ComputerError::NotFound)?;
        Ok(OperationLookup {
            id,
            computer: ComputerId::try_from(row.computer_id)
                .map_err(|_| ComputerError::Unavailable)?,
            action: serde_json::from_value(row.action.into())
                .map_err(|_| ComputerError::Unavailable)?,
            grant: row
                .automation_grant_id
                .map(AutomationGrantId::try_from)
                .transpose()
                .map_err(|_| ComputerError::Unavailable)?,
            participant,
        })
    }

    pub(crate) async fn read_admitted_operation(
        &self,
        caller: &TaskOwner,
        lookup: OperationLookup,
        retained_owner: &TaskOwner,
    ) -> Result<Operation> {
        let mut params = crate::store::owner_query_bindings(caller)?;
        let serde_json::Value::String(action) =
            serde_json::to_value(lookup.action).map_err(|_| ComputerError::Unavailable)?
        else {
            return Err(ComputerError::Unavailable);
        };
        params.extend([
            (
                "operation",
                crate::operation_admission::operation_record(lookup.id).into_value(),
            ),
            (
                "provider",
                self.provider_instance_id.into_uuid().into_value(),
            ),
            ("as_owner", lookup.participant.is_owner().into_value()),
            ("computer_id", lookup.computer.into_uuid().into_value()),
            ("action", action.into_value()),
            (
                "grant_id",
                lookup.grant.map(AutomationGrantId::into_uuid).into_value(),
            ),
            (
                "retained_owner",
                crate::session_grants::object(retained_owner)?.into_value(),
            ),
        ]);
        let mut response = self
            .query(include_str!("../queries/admitted_operation.surql"), params)
            .await?;
        let row: Option<crate::operation::OperationRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let operation = Operation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if operation.operation_id != lookup.id {
            return Err(ComputerError::Unavailable);
        }
        Ok(operation)
    }
}
