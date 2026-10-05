//! Owner-checked provider associations attached to immutable Task contribution receipts.
use crate::{OwnedTaskTable, TaskContribution, TaskError};
use chrono::{DateTime, Utc};
use surrealdb::{
    Connection,
    method::Query,
    types::{SurrealValue, Value},
};

/// A provider identity projection produced by a registered owner adapter.
/// The creation receipt is compared in full; only the separate association changes.
#[derive(Clone)]
pub struct TaskAssociation {
    receipt: TaskContribution,
    association: Value,
}
impl TaskAssociation {
    pub fn associate<I: SurrealValue, A: SurrealValue>(
        table: OwnedTaskTable,
        identity: I,
        association: A,
    ) -> Result<Self, TaskError> {
        let association = association.into_value();
        if !matches!(association, Value::Object(_)) {
            return Err(TaskError::InvalidRecord(
                "Task provider association must be a typed object".into(),
            ));
        }
        Ok(Self {
            receipt: TaskContribution::create(table, identity)?,
            association,
        })
    }
    pub(crate) fn check_table(&self, table: &OwnedTaskTable) -> Result<(), TaskError> {
        self.receipt.check_table(table)
    }
    pub(crate) fn sql(&self) -> &'static str {
        include_str!("../queries/contribution_associate.surql")
    }
    pub(crate) fn bind<'q, C: Connection>(
        &self,
        query: Query<'q, C>,
        task: veoveo_types::TaskId,
        task_type: &veoveo_types::TaskTypeName,
        created_at: DateTime<Utc>,
    ) -> Query<'q, C> {
        self.receipt
            .bind(query, task, task_type, created_at)
            .bind(("_contribution_association", self.association.clone()))
    }
}

/// Owner-authored dispatch metadata, separate from both creation and provider association.
/// Preparing an identical receipt is recovery evidence; it never grants a second send.
#[derive(Clone)]
pub struct TaskDispatch {
    receipt: TaskContribution,
    metadata: Value,
}
impl TaskDispatch {
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }
    pub fn prepare<I: SurrealValue, D: SurrealValue>(
        table: OwnedTaskTable,
        identity: I,
        metadata: D,
    ) -> Result<Self, TaskError> {
        let metadata = metadata.into_value();
        if !matches!(metadata, Value::Object(_)) {
            return Err(TaskError::InvalidRecord(
                "Task dispatch metadata must be a typed object".into(),
            ));
        }
        Ok(Self {
            receipt: TaskContribution::create(table, identity)?,
            metadata,
        })
    }
    pub(crate) fn check_table(&self, table: &OwnedTaskTable) -> Result<(), TaskError> {
        self.receipt.check_table(table)
    }
    pub(crate) fn sql(&self) -> &'static str {
        include_str!("../queries/contribution_dispatch.surql")
    }
    pub(crate) fn bind<'q, C: Connection>(
        &self,
        query: Query<'q, C>,
        task: veoveo_types::TaskId,
        task_type: &veoveo_types::TaskTypeName,
        created_at: DateTime<Utc>,
    ) -> Query<'q, C> {
        self.receipt
            .bind(query, task, task_type, created_at)
            .bind(("_contribution_dispatch", self.metadata.clone()))
    }
}
/// Only NewlyPrepared authorizes the original worker's one submission attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchPreparation {
    NewlyPrepared,
    AlreadyPrepared,
}
