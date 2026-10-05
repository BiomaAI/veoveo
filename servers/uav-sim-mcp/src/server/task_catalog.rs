//! UAV-owned lookup rows written atomically with Task creation and settlement.
use crate::contract::{ExecuteVehicleMissionPlanRequest, MissionPlanId, UavTaskKind};
use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_modules::TableName;
use veoveo_task_runtime::{
    OwnedTaskTable, TaskContribution, TaskContributions, TaskCreation, TaskError, TaskRuntime,
    TaskSettlement, TaskSnapshot,
};
use veoveo_types::{PrincipalId, TaskTypeDefinition, TaskTypeName};

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub(super) struct Identity {
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub owner: RecordId,
    pub profile: RecordId,
    #[surreal(wrap)]
    pub principal_key: PrincipalId,
    #[surreal(wrap)]
    pub plan_id: Option<MissionPlanId>,
    pub expected_revision: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Terminal {
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(Debug, SurrealValue)]
struct Settlement {
    #[surreal(wrap)]
    status: Terminal,
    completed_at: DateTime<Utc>,
}
pub(super) struct UavTaskContributions {
    table: OwnedTaskTable,
    kinds: Vec<TaskTypeName>,
}
impl UavTaskContributions {
    pub(super) fn bind(runtime: TaskRuntime) -> Result<TaskRuntime, TaskError> {
        let setup = crate::schema::ownership()
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
        let table = OwnedTaskTable::new(
            &setup,
            TableName::new("uav_task")
                .map_err(|error| TaskError::InvalidRecord(error.to_string()))?,
        )?;
        runtime
            .requiring_contributions(UavTaskKind::ALL.iter().map(|kind| kind.name()))?
            .bind_contributions(std::sync::Arc::new(Self {
                table,
                kinds: UavTaskKind::ALL.iter().map(|kind| kind.name()).collect(),
            }))
    }
}
fn identity(
    owner: &veoveo_task_runtime::TaskOwner,
    kind: &TaskTypeName,
    request: &serde_json::Value,
) -> Result<Identity, TaskError> {
    let (plan_id, expected_revision) = if *kind == UavTaskKind::ExecuteMission.name() {
        let request: ExecuteVehicleMissionPlanRequest = serde_json::from_value(request.clone())?;
        if request.expected_revision > i64::MAX as u64 {
            return Err(TaskError::InvalidRecord(
                "UAV mission revision exceeds native revision range".into(),
            ));
        }
        (Some(request.plan_id), Some(request.expected_revision))
    } else {
        let operation: crate::contract::DurableOperation = serde_json::from_value(request.clone())?;
        if operation.task_type() != *kind {
            return Err(TaskError::InvalidRecord(
                "UAV request and Task operation disagree".into(),
            ));
        }
        (None, None)
    };
    Ok(Identity {
        tenant: veoveo_platform_store::deterministic_tenant_id(owner.authority.tenant.as_str())?
            .record_id(),
        work_context: veoveo_platform_store::deterministic_work_context_id(
            owner.authority.tenant.as_str(),
            owner.authority.work_context.as_str(),
        )?
        .record_id(),
        owner: veoveo_platform_store::deterministic_principal_id(
            owner.tenant_key(),
            &owner.principal_key,
        )?
        .record_id(),
        profile: RecordId::new("profile", owner.profile.clone()),
        principal_key: owner
            .principal_key
            .parse()
            .map_err(|error| TaskError::InvalidRecord(format!("invalid UAV principal: {error}")))?,
        plan_id,
        expected_revision,
    })
}
impl TaskContributions for UavTaskContributions {
    fn table(&self) -> &OwnedTaskTable {
        &self.table
    }
    fn task_types(&self) -> &[TaskTypeName] {
        &self.kinds
    }
    fn created(&self, creation: TaskCreation<'_>) -> Result<TaskContribution, TaskError> {
        TaskContribution::create(
            self.table.clone(),
            identity(
                &creation.draft.owner,
                &creation.draft.task_type,
                &creation.draft.request,
            )?,
        )
    }
    fn settled(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        completed_at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        let status = match settlement {
            TaskSettlement::Succeeded { .. } => Terminal::Succeeded,
            TaskSettlement::Failed { .. } => Terminal::Failed,
            TaskSettlement::Cancelled => Terminal::Cancelled,
        };
        TaskContribution::settle(
            self.table.clone(),
            identity(&current.owner, &current.task_type, &current.request)?,
            Settlement {
                status,
                completed_at,
            },
        )
    }
}

#[derive(SurrealValue)]
pub(super) struct CatalogRow {
    pub id: RecordId,
    pub task: RecordId,
    #[surreal(wrap)]
    pub task_type: TaskTypeName,
    pub created_at: DateTime<Utc>,
    pub identity: Identity,
    settlement: Option<Settlement>,
}
impl CatalogRow {
    pub fn task_id(&self) -> anyhow::Result<veoveo_types::TaskId> {
        anyhow::ensure!(
            self.task.table.as_str() == "task",
            "UAV lookup has a foreign Task link"
        );
        let surrealdb::types::RecordIdKey::Uuid(key) = &self.task.key else {
            anyhow::bail!("UAV lookup Task link is not a native UUID");
        };
        let task = veoveo_types::TaskId::from_uuid((*key).into());
        anyhow::ensure!(
            self.id == RecordId::new("uav_task", task.to_string()),
            "UAV lookup row and Task identity disagree"
        );
        Ok(task)
    }
    pub fn check(&self, task: &TaskSnapshot) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.task_id()? == task.task_id
                && self.task_type == task.task_type
                && self.created_at == task.created_at,
            "UAV lookup and Task metadata disagree"
        );
        anyhow::ensure!(
            self.identity == identity(&task.owner, &task.task_type, &task.request)?,
            "UAV lookup and retained Task request disagree"
        );
        match (&self.settlement, task.status) {
            (
                None,
                veoveo_task_runtime::TaskStatus::Queued
                | veoveo_task_runtime::TaskStatus::Running
                | veoveo_task_runtime::TaskStatus::Waiting
                | veoveo_task_runtime::TaskStatus::CancelRequested,
            ) => {}
            (Some(settlement), status) => {
                let expected = match status {
                    veoveo_task_runtime::TaskStatus::Succeeded => Terminal::Succeeded,
                    veoveo_task_runtime::TaskStatus::Failed => Terminal::Failed,
                    veoveo_task_runtime::TaskStatus::Cancelled => Terminal::Cancelled,
                    _ => anyhow::bail!("UAV lookup settlement disagrees with nonterminal Task"),
                };
                anyhow::ensure!(
                    settlement.status == expected
                        && Some(settlement.completed_at) == task.completed_at,
                    "UAV lookup settlement and Task completion disagree"
                );
            }
            _ => anyhow::bail!("terminal UAV Task has no lookup settlement"),
        }
        Ok(())
    }
}

pub(super) fn expected_identity(task: &TaskSnapshot) -> Result<Identity, TaskError> {
    identity(&task.owner, &task.task_type, &task.request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, time::Duration};
    use surrealdb::types::{RecordId, Value};
    use veoveo_task_runtime::{CreateTask, RecoveryClass};
    use veoveo_types::TaskId;

    #[tokio::test]
    async fn lookup_writes_are_closed_and_creation_conflicts_roll_back_the_task() {
        tokio::time::timeout(Duration::from_secs(180), async {
            let db = crate::server::test_support::database(
                crate::server::test_support::fixture::StoreBackend::Memory,
            )
            .await;
            let owner = crate::server::ownership::runtime_owner(
                &crate::server::test_support::identity("test", "mission", "pilot", &[]),
            );
            let runtime = UavTaskContributions::bind(TaskRuntime::new(
                db.a.clone(),
                "uav-sim",
                "lookup-test",
            ))
            .unwrap();
            let create = |task_id| CreateTask {
                task_id,
                owner: owner.clone(),
                server: "uav-sim".into(),
                task_type: UavTaskKind::ExecuteMission.name(),
                request: serde_json::json!({"plan_id":"native-plan","expected_revision":1}),
                recovery_class: RecoveryClass::InterruptedIndeterminate,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: None,
                retention_pins: BTreeSet::new(),
            };
            let task = runtime
                .create(create(TaskId::new()))
                .await
                .unwrap()
                .snapshot;
            let mut scope_probe =
                db.a.client()
                    .query(include_str!(
                        "queries/task_catalog/tests/where_return_scope.surql"
                    ))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let escaped: Value = scope_probe.take(0).unwrap();
            let local: Value = scope_probe.take(1).unwrap();
            assert_eq!(
                escaped,
                Value::Bool(true),
                "WHERE RETURN must expose its escaping scalar in this pinned native control"
            );
            assert!(
                matches!(local, Value::Array(ref rows) if rows.len() == 1),
                "WHERE final expression must preserve selected rows: {local:?}"
            );
            let catalog = RecordId::new("uav_task", task.task_id.to_string());
            let before: Value =
                db.a.client()
                    .query(include_str!("queries/task_catalog/tests/row.surql"))
                    .bind(("catalog", catalog.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap()
                    .take(0)
                    .unwrap();
            for (case, settlement) in [
                serde_json::json!({}),
                serde_json::json!({"status":"succeeded"}),
                serde_json::json!({"completed_at":"not-a-date"}),
            ]
            .into_iter()
            .enumerate()
            {
                let Value::Object(mut invalid) = before.clone() else {
                    panic!("lookup missing");
                };
                invalid.insert("settlement", settlement.into_value());
                assert!(
                    db.a.client()
                        .query(include_str!("queries/task_catalog/tests/replace.surql"))
                        .bind(("catalog", catalog.clone()))
                        .bind(("row", Value::Object(invalid)))
                        .await
                        .unwrap()
                        .check()
                        .is_err(),
                    "invalid settlement case {case} was admitted"
                );
                let actual: Value =
                    db.a.client()
                        .query(include_str!("queries/task_catalog/tests/row.surql"))
                        .bind(("catalog", catalog.clone()))
                        .await
                        .unwrap()
                        .check()
                        .unwrap()
                        .take(0)
                        .unwrap();
                assert_eq!(before, actual);
            }
            let conflicting = TaskId::new();
            let conflicting_catalog = RecordId::new("uav_task", conflicting.to_string());
            let Value::Object(mut row) = before else {
                unreachable!()
            };
            row.remove("id");
            row.insert(
                "task",
                veoveo_platform_store::task_record_id(conflicting).into_value(),
            );
            db.a.client()
                .query(include_str!("queries/task_catalog/tests/create.surql"))
                .bind(("catalog", conflicting_catalog))
                .bind(("row", Value::Object(row)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(runtime.create(create(conflicting)).await.is_err());
            assert!(runtime.get(conflicting).await.unwrap().is_none());
        })
        .await
        .unwrap();
    }
}
