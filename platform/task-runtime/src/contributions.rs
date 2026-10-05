//! Pure module-owned row contributions composed into kernel Task transactions.
use crate::{CreateTask, TaskError, TaskFailure, TaskRuntime, TaskSnapshot};
use chrono::{DateTime, Utc};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use surrealdb::{
    Connection,
    method::Query,
    types::{RecordId, SurrealValue, Value},
};
use veoveo_modules::{ModuleLayer, ModuleOwnership, OwnershipClaim, TableName};
use veoveo_types::TaskTypeName;

/// A table admitted by a trusted module ownership declaration.
/// Global ownership conflicts and installed readiness belong to composition.
#[derive(Clone, Debug)]
pub struct OwnedTaskTable(TableName);
impl OwnedTaskTable {
    pub fn new(module: &ModuleOwnership, table: TableName) -> Result<Self, TaskError> {
        let claimed = module.ownership().iter().any(|claim| match claim {
            OwnershipClaim::Table(name) => name == &table,
            OwnershipClaim::TablePrefix(prefix) => table.as_str().starts_with(prefix.as_str()),
            _ => false,
        });
        if module.layer() != ModuleLayer::Optional || table.as_str() == "task" || !claimed {
            return Err(TaskError::InvalidRecord("Task contribution target must be a declared optional-module table distinct from the kernel Task table".into()));
        }
        Ok(Self(table))
    }
}

pub struct TaskCreation<'a> {
    pub draft: &'a CreateTask,
    pub created_at: DateTime<Utc>,
}

pub enum TaskSettlement<'a> {
    Succeeded { result: &'a serde_json::Value },
    Failed { failure: &'a TaskFailure },
    Cancelled,
}

/// Adapters compute values only; they receive neither a connection nor SQL authority.
pub trait TaskContributions: Send + Sync {
    fn task_types(&self) -> &[TaskTypeName];
    fn table(&self) -> &OwnedTaskTable;
    fn created(&self, creation: TaskCreation<'_>) -> Result<TaskContribution, TaskError>;
    fn dispatch_prepared(
        &self,
        _current: &TaskSnapshot,
        _dispatch: &crate::TaskDispatch,
    ) -> Result<crate::TaskDispatch, TaskError> {
        Err(TaskError::InvalidRecord(
            "Task owner does not support dispatch preparation".into(),
        ))
    }
    fn provider_associated(
        &self,
        _current: &TaskSnapshot,
        _binding: &veoveo_platform_store::WebhookJobBinding,
    ) -> Result<crate::TaskAssociation, TaskError> {
        Err(TaskError::InvalidRecord(
            "Task owner does not support provider associations".into(),
        ))
    }
    fn settled(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        completed_at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError>;
}

/// One immutable identity row per Task/table and a checked terminal settlement.
#[derive(Clone, Default)]
pub struct TaskContribution(Option<RowMutation>);
#[derive(Clone)]
struct RowMutation {
    table: OwnedTaskTable,
    identity: Value,
    settlement: Option<Value>,
}
impl TaskContribution {
    pub fn none() -> Self {
        Self::default()
    }
    pub fn create<T: SurrealValue>(table: OwnedTaskTable, identity: T) -> Result<Self, TaskError> {
        Self::new(table, identity.into_value(), None)
    }
    pub fn settle<I: SurrealValue, S: SurrealValue>(
        table: OwnedTaskTable,
        identity: I,
        settlement: S,
    ) -> Result<Self, TaskError> {
        Self::new(table, identity.into_value(), Some(settlement.into_value()))
    }
    fn new(
        table: OwnedTaskTable,
        identity: Value,
        settlement: Option<Value>,
    ) -> Result<Self, TaskError> {
        if !matches!(identity, Value::Object(_))
            || settlement
                .as_ref()
                .is_some_and(|v| !matches!(v, Value::Object(_)))
        {
            return Err(TaskError::InvalidRecord(
                "Task contribution identity and settlement must be typed objects".into(),
            ));
        }
        Ok(Self(Some(RowMutation {
            table,
            identity,
            settlement,
        })))
    }
    pub(crate) fn check_table(&self, table: &OwnedTaskTable) -> Result<(), TaskError> {
        if self.0.as_ref().is_some_and(|row| row.table.0 != table.0) {
            return Err(TaskError::InvalidRecord(
                "Task contribution target differs from bound adapter table".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn sql(&self, creation: bool) -> Result<&'static str, TaskError> {
        let Some(row) = &self.0 else {
            return Ok("");
        };
        if creation != row.settlement.is_none() {
            return Err(TaskError::InvalidRecord(
                "Task contribution mutation does not match lifecycle phase".into(),
            ));
        }
        Ok(if creation {
            include_str!("../queries/contribution_create.surql")
        } else {
            include_str!("../queries/contribution_settle.surql")
        })
    }
    pub(crate) fn bind<'q, C: Connection>(
        &self,
        query: Query<'q, C>,
        task: veoveo_types::TaskId,
        task_type: &TaskTypeName,
        created_at: DateTime<Utc>,
    ) -> Query<'q, C> {
        let Some(row) = &self.0 else {
            return query;
        };
        query
            .bind((
                "_contribution_row",
                RecordId::new(row.table.0.as_str(), task.to_string()),
            ))
            .bind((
                "_contribution_task",
                veoveo_platform_store::task_record_id(task),
            ))
            .bind(("_contribution_type", task_type.to_string()))
            .bind(("_contribution_created", created_at))
            .bind(("_contribution_identity", row.identity.clone()))
            .bind(("_contribution_settlement", row.settlement.clone()))
    }
}

#[derive(Clone, Default)]
pub(crate) struct ContributionRegistry {
    required: BTreeSet<TaskTypeName>,
    adapters: BTreeMap<TaskTypeName, Arc<dyn TaskContributions>>,
}
impl TaskRuntime {
    pub fn requiring_contributions(
        mut self,
        operations: impl IntoIterator<Item = TaskTypeName>,
    ) -> Result<Self, TaskError> {
        self.contributions.required.extend(operations);
        Ok(self)
    }
    pub fn bind_contributions(
        mut self,
        adapter: Arc<dyn TaskContributions>,
    ) -> Result<Self, TaskError> {
        let operations = adapter.task_types();
        if operations.is_empty()
            || operations.iter().collect::<BTreeSet<_>>().len() != operations.len()
            || operations
                .iter()
                .any(|name| self.contributions.adapters.contains_key(name))
            || self
                .contributions
                .adapters
                .values()
                .any(|bound| bound.table().0 == adapter.table().0)
        {
            return Err(TaskError::InvalidRecord(
                "Task contribution binding requires distinct unbound operation names".into(),
            ));
        }
        for name in operations {
            self.contributions
                .adapters
                .insert(name.clone(), adapter.clone());
        }
        Ok(self)
    }
    pub fn require_contribution(&self, operation: &TaskTypeName) -> Result<(), TaskError> {
        if !self.contributions.adapters.contains_key(operation) {
            return Err(TaskError::ContributionUnbound(operation.clone()));
        }
        Ok(())
    }
    pub(crate) fn check_required_contributions(&self) -> Result<(), TaskError> {
        for operation in &self.contributions.required {
            self.require_contribution(operation)?;
        }
        Ok(())
    }
    pub(crate) fn check_contribution(&self, operation: &TaskTypeName) -> Result<(), TaskError> {
        if self.contributions.required.contains(operation) {
            self.require_contribution(operation)?;
        }
        Ok(())
    }
    pub(crate) fn creation_contribution(
        &self,
        draft: &CreateTask,
        created_at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        self.check_contribution(&draft.task_type)?;
        let Some(adapter) = self.contributions.adapters.get(&draft.task_type) else {
            return Ok(TaskContribution::none());
        };
        let contribution = adapter.created(TaskCreation { draft, created_at })?;
        contribution.check_table(adapter.table())?;
        Ok(contribution)
    }
    pub(crate) fn dispatch_contribution(
        &self,
        current: &TaskSnapshot,
        dispatch: &crate::TaskDispatch,
    ) -> Result<crate::TaskDispatch, TaskError> {
        let adapter = self
            .contributions
            .adapters
            .get(&current.task_type)
            .ok_or_else(|| TaskError::ContributionUnbound(current.task_type.clone()))?;
        let admitted = adapter.dispatch_prepared(current, dispatch)?;
        admitted.check_table(adapter.table())?;
        Ok(admitted)
    }
    pub(crate) fn provider_association(
        &self,
        current: &TaskSnapshot,
        binding: &veoveo_platform_store::WebhookJobBinding,
    ) -> Result<crate::TaskAssociation, TaskError> {
        let adapter = self
            .contributions
            .adapters
            .get(&current.task_type)
            .ok_or_else(|| TaskError::ContributionUnbound(current.task_type.clone()))?;
        let association = adapter.provider_associated(current, binding)?;
        association.check_table(adapter.table())?;
        Ok(association)
    }
    pub(crate) fn settlement_contribution(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        completed_at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        self.check_contribution(&current.task_type)?;
        let Some(adapter) = self.contributions.adapters.get(&current.task_type) else {
            return Ok(TaskContribution::none());
        };
        let contribution = adapter.settled(current, settlement, completed_at)?;
        contribution.check_table(adapter.table())?;
        Ok(contribution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_modules::{ModuleName, OwnershipClaim};
    #[derive(SurrealValue)]
    struct Identity {
        revision: i64,
    }
    fn table() -> OwnedTaskTable {
        let module = ModuleOwnership::new(
            ModuleName::new("frames").unwrap(),
            ModuleLayer::Optional,
            vec![OwnershipClaim::Table(
                TableName::new("coordinate_operation").unwrap(),
            )],
        )
        .unwrap();
        OwnedTaskTable::new(&module, TableName::new("coordinate_operation").unwrap()).unwrap()
    }
    #[test]
    fn contributions_admit_objects_and_exact_nonprefix_ownership_only() {
        assert!(TaskContribution::create(table(), 42_i64).is_err());
        let creation = TaskContribution::create(table(), Identity { revision: 1 }).unwrap();
        assert!(creation.sql(false).is_err());
        let settlement =
            TaskContribution::settle(table(), Identity { revision: 1 }, Identity { revision: 2 })
                .unwrap();
        assert!(settlement.sql(true).is_err());
        assert!(TaskContribution::none().sql(true).unwrap().is_empty());
        let kernel = ModuleOwnership::new(
            ModuleName::new("tasks").unwrap(),
            ModuleLayer::Kernel,
            vec![OwnershipClaim::Table(TableName::new("task").unwrap())],
        )
        .unwrap();
        assert!(OwnedTaskTable::new(&kernel, TableName::new("task").unwrap()).is_err());
        let forged = ModuleOwnership::new(
            ModuleName::new("optional").unwrap(),
            ModuleLayer::Optional,
            vec![OwnershipClaim::Table(TableName::new("task").unwrap())],
        )
        .unwrap();
        assert!(OwnedTaskTable::new(&forged, TableName::new("task").unwrap()).is_err());
    }
}
