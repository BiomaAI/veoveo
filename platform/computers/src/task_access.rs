//! Private command/file lookup and admission, shared by their Task projections.
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::{
        Action, AutomationGrantId, AutomationPermission, ComputerId, ExecutionId, FileTransferId,
    },
};
use std::time::Instant;
use surrealdb::types::{RecordId, SurrealValue, Value};
use uuid::Uuid;

pub(crate) enum TaskSelection {
    Command(ExecutionId),
    File(FileTransferId),
}
impl TaskSelection {
    fn record(&self) -> RecordId {
        match self {
            Self::Command(id) => RecordId::new(
                "computer_execution",
                surrealdb::types::Uuid::from(id.into_uuid()),
            ),
            Self::File(id) => RecordId::new(
                "computer_file_transfer",
                surrealdb::types::Uuid::from(id.into_uuid()),
            ),
        }
    }
}
#[derive(SurrealValue)]
struct Lookup {
    computer_id: Uuid,
    owner_key: String,
    grant_id: Option<String>,
    owned: bool,
}

pub(crate) struct TaskReadPermit {
    selection: TaskSelection,
    computer: ComputerId,
    owner_key: String,
    grant: Option<AutomationGrantId>,
    actor_key: String,
    owned: bool,
    labels: Vec<String>,
    deadline: Instant,
    can_cancel: bool,
}
impl TaskReadPermit {
    pub fn valid_until(&self) -> Instant {
        self.deadline
    }
    pub fn can_cancel(&self) -> bool {
        self.can_cancel
    }
    pub fn bindings(&self, provider: Uuid) -> Result<Vec<(&'static str, Value)>> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(ComputerError::Forbidden)?;
        let end = chrono::Utc::now()
            + chrono::TimeDelta::from_std(remaining).map_err(|_| ComputerError::Unavailable)?;
        Ok(vec![
            ("execution", self.selection.record().into_value()),
            ("provider", provider.into_value()),
            ("computer_id", self.computer.into_uuid().into_value()),
            ("owner_key", self.owner_key.clone().into_value()),
            ("grant_id", self.grant.map(|id| id.to_string()).into_value()),
            ("actor_key", self.actor_key.clone().into_value()),
            ("owned", self.owned.into_value()),
            ("admission_end", end.into_value()),
            (
                "file",
                matches!(self.selection, TaskSelection::File(_)).into_value(),
            ),
            ("labels", self.labels.clone().into_value()),
        ])
    }
}
impl ComputersStore {
    pub(crate) async fn admit_task_metadata(
        &self,
        actor: &ComputerActor,
        selection: TaskSelection,
        actor_key: String,
        cancel: bool,
    ) -> Result<TaskReadPermit> {
        actor.check_admission()?;
        let file = matches!(selection, TaskSelection::File(_));
        let mut params = crate::store::owner_query_bindings(actor.owner())?;
        params.extend(crate::computer_access::scope(actor.accepted())?);
        params.extend([
            ("execution", selection.record().into_value()),
            ("provider", self.provider_instance_id.into_value()),
            ("actor_key", actor_key.clone().into_value()),
            ("file", file.into_value()),
            (
                "direct_owner",
                (actor.accepted().actor.id == actor.accepted().request_context.principal.id)
                    .into_value(),
            ),
        ]);
        let mut read = self
            .query(include_str!("../queries/task_lookup.surql"), params)
            .await?;
        let index = read
            .num_statements()
            .checked_sub(1)
            .ok_or(ComputerError::Unavailable)?;
        let row: Option<Lookup> = read.take(index).map_err(|_| ComputerError::Unavailable)?;
        let row = row.ok_or(ComputerError::NotFound)?;
        let computer =
            ComputerId::try_from(row.computer_id).map_err(|_| ComputerError::Unavailable)?;
        let grant = row
            .grant_id
            .map(|id| id.parse::<AutomationGrantId>())
            .transpose()
            .map_err(|_| ComputerError::Unavailable)?;
        let (deadline, can_cancel) = if row.owned {
            let control = self.control_authority(actor).await?;
            control.require_read(Some(computer))?;
            if cancel {
                control.require_action(Action::Stop)?;
            }
            (control.valid_until(), control.allows_action(Action::Stop))
        } else {
            let authority = self
                .authorize_automation_grant(
                    actor,
                    computer,
                    grant.ok_or(ComputerError::NotFound)?,
                    AutomationPermission::Execute,
                )
                .await?;
            if file {
                authority.require_file_transfer()?;
            }
            if crate::identity::owner_key(&authority.computer()?.owner)? != row.owner_key {
                return Err(ComputerError::NotFound);
            }
            (authority.valid_until(), true)
        };
        Ok(TaskReadPermit {
            selection,
            computer,
            owner_key: row.owner_key,
            grant,
            actor_key,
            owned: row.owned,
            labels: actor
                .owner()
                .data_labels
                .iter()
                .filter(|label| {
                    actor
                        .accepted()
                        .request_context
                        .principal
                        .data_labels
                        .iter()
                        .any(|source| source.as_str() == label.as_str())
                })
                .cloned()
                .collect(),
            deadline,
            can_cancel,
        })
    }
}
