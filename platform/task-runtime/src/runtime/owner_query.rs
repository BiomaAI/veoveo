//! One owner and operation selection shared by reads, pages and subscriptions.
use std::collections::BTreeSet;

use surrealdb::{Connection, method::Query};
use veoveo_types::TaskTypeName;

use super::{TaskRuntime, owner_reads::OwnerScope};
use crate::{TaskError, TaskOwner};

/// A current-owner Task query. Domains restrict operations before any records decode.
/// The default selects all operation types hosted by this runtime's server.
#[derive(Clone)]
pub struct OwnerTaskQuery {
    pub(crate) runtime: TaskRuntime,
    pub(super) owner: TaskOwner,
    task_types: Option<BTreeSet<TaskTypeName>>,
}

impl TaskRuntime {
    pub fn for_owner(&self, owner: &TaskOwner) -> OwnerTaskQuery {
        OwnerTaskQuery {
            runtime: self.clone(),
            owner: owner.clone(),
            task_types: None,
        }
    }
}

impl OwnerTaskQuery {
    /// Select one declared or validated operation type.
    /// ```compile_fail
    /// use veoveo_task_runtime::{TaskRuntime, TaskOwner};
    /// fn query(runtime: &TaskRuntime, owner: &TaskOwner) {
    ///     runtime.for_owner(owner).of_type("untyped-operation");
    /// }
    /// ```
    pub fn of_type(mut self, kind: TaskTypeName) -> Self {
        self.task_types = Some(BTreeSet::from([kind]));
        self
    }

    /// Select 1–32 names, replacing the previous selection. Empty means an error,
    /// never an unrestricted query. Duplicates count toward the admission limit.
    pub fn of_types(
        mut self,
        kinds: impl IntoIterator<Item = TaskTypeName>,
    ) -> Result<Self, TaskError> {
        let kinds = kinds.into_iter().take(33).collect::<Vec<_>>();
        if !(1..=32).contains(&kinds.len()) {
            return Err(TaskError::InvalidPageQuery);
        }
        self.task_types = Some(kinds.into_iter().collect());
        Ok(self)
    }

    pub(super) fn type_predicate(&self) -> &'static str {
        if self.task_types.is_some() {
            "AND task_type IN $task_types"
        } else {
            ""
        }
    }

    pub(super) fn bind<'a, C: Connection>(
        &self,
        query: Query<'a, C>,
    ) -> Result<Query<'a, C>, TaskError> {
        Ok(OwnerScope::new(&self.runtime, &self.owner)?
            .bind(query)
            .bind((
                "task_types",
                self.task_types
                    .as_ref()
                    .map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>()),
            )))
    }
}
