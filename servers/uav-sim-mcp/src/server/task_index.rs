//! Authorized task and mission indexes; SQL performs every persisted-row selection.
use crate::contract::UavTaskKind;
use anyhow::Result;
use surrealdb::types::SurrealValue;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{
    PlatformStore, RecordId, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id,
};
use veoveo_task_runtime::{TaskPageCursor, TaskRuntime, TaskSnapshot};
use veoveo_types::{TaskId, TaskTypeDefinition};

use super::{index, ownership::runtime_owner};
use crate::{
    contract::{CollectionPage, MissionId, UavMissionCursor, UavUsageCursor, UavUsagePosition},
    uris,
};

// Bind fields as individual parameters so the database can plan indexed equality reads.
#[derive(SurrealValue)]
struct Scope {
    server: RecordId,
    tenant: RecordId,
    owner: RecordId,
    profile: RecordId,
    context: RecordId,
    #[surreal(wrap)]
    tenant_key: Option<veoveo_types::TenantId>,
    #[surreal(wrap)]
    principal_key: veoveo_types::PrincipalId,
    #[surreal(wrap)]
    profile_key: veoveo_types::GatewayProfileId,
    #[surreal(wrap)]
    context_key: veoveo_types::WorkContextId,
    #[surreal(wrap)]
    authority_tenant: veoveo_types::TenantId,
    #[surreal(wrap)]
    data_labels: Vec<veoveo_types::DataLabelId>,
    #[surreal(wrap)]
    mission_task_type: veoveo_types::TaskTypeName,
    #[surreal(wrap)]
    types: Vec<veoveo_types::TaskTypeName>,
}

fn scope(identity: &GatewayInternalIdentity) -> Result<Scope> {
    let owner = runtime_owner(identity);
    Ok(Scope {
        server: RecordId::new("mcp_server", "uav-sim"),
        tenant: deterministic_tenant_id(owner.tenant_key())?.record_id(),
        owner: deterministic_principal_id(owner.tenant_key(), &owner.principal_key)?.record_id(),
        profile: RecordId::new("profile", owner.profile.clone()),
        context: deterministic_work_context_id(
            identity.authority.tenant.as_str(),
            identity.authority.work_context.as_str(),
        )?
        .record_id(),
        tenant_key: owner.tenant_key.map(|key| key.parse()).transpose()?,
        principal_key: owner.principal_key.parse()?,
        profile_key: owner.profile.parse()?,
        context_key: identity.authority.work_context.clone(),
        authority_tenant: identity.authority.tenant.clone(),
        data_labels: owner
            .data_labels
            .into_iter()
            .map(|label| label.parse())
            .collect::<Result<_, _>>()?,
        mission_task_type: UavTaskKind::ExecuteMission.name(),
        types: UavTaskKind::ALL.iter().map(|kind| kind.name()).collect(),
    })
}

pub(super) async fn usage_page(
    tasks: &TaskRuntime,
    identity: &GatewayInternalIdentity,
    after: Option<&UavUsagePosition>,
) -> Result<CollectionPage<String>> {
    if let Some(cursor) = after {
        anyhow::ensure!(
            cursor.task_id.as_uuid().get_version_num() == 7,
            "invalid task cursor ID"
        );
    }
    let after = after.map(|position| TaskPageCursor {
        created_at: position.created_at,
        task_id: position.task_id,
    });
    let page = tasks
        .for_owner(&runtime_owner(identity))
        .of_types(UavTaskKind::ALL.iter().map(|kind| kind.name()))?
        .page(after.as_ref(), index::PAGE_SIZE)
        .await?;
    Ok(CollectionPage {
        items: page
            .items
            .into_iter()
            .map(|task| uris::usage_task(task.task_id).map(String::from))
            .collect::<Result<_, _>>()?,
        limit: index::PAGE_SIZE,
        next_cursor: page
            .next_cursor
            .map(|cursor| {
                UavUsageCursor::new(UavUsagePosition {
                    created_at: cursor.created_at,
                    task_id: cursor.task_id,
                })
                .map(|cursor| cursor.as_str().to_owned())
            })
            .transpose()?,
    })
}

pub(super) async fn task(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    id: TaskId,
) -> Result<Option<TaskSnapshot>> {
    selected_task(store, identity, Some(id), None).await
}
pub(super) async fn mission(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    id: &MissionId,
) -> Result<Option<TaskSnapshot>> {
    selected_task(store, identity, None, Some(id.clone())).await
}
async fn selected_task(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    task: Option<TaskId>,
    mission: Option<MissionId>,
) -> Result<Option<TaskSnapshot>> {
    let identity = identity.clone();
    let runtime = TaskRuntime::new(store.clone(), "uav-sim", "catalog-read");
    veoveo_platform_store::read_transaction::read(store.client(), move |transaction| {
        Box::pin(async move {
            let sql = if task.is_some() {
                include_str!("queries/task_index/task.surql")
            } else {
                include_str!("queries/task_index/mission.surql")
            };
            let mut response = transaction
                .query(sql)
                .bind(scope(&identity)?)
                .bind(("task", task.map(veoveo_platform_store::task_record_id)))
                .bind(("mission", mission.map(|id| id.to_string())))
                .await?
                .check()?;
            let rows: Vec<super::task_catalog::CatalogRow> =
                response.take(if task.is_some() { 0 } else { 2 })?;
            let Some(row) = rows.into_iter().next() else {
                return Ok(None);
            };
            let mut tasks = runtime
                .for_owner(&runtime_owner(&identity))
                .of_types(UavTaskKind::ALL.iter().map(|kind| kind.name()))?
                .get_many_in(transaction, &[row.task_id()?])
                .await?;
            let Some(task) = tasks.pop() else {
                return Ok(None);
            };
            row.check(&task)?;
            Ok(Some(task))
        })
    })
    .await
}

#[cfg(test)]
#[derive(Debug, SurrealValue)]
pub(super) struct ExplainNode {
    pub operator: String,
    #[surreal(default)]
    pub attributes: std::collections::BTreeMap<String, String>,
    #[surreal(default)]
    pub children: Vec<ExplainNode>,
}
#[cfg(test)]
impl ExplainNode {
    pub fn any(&self, predicate: impl Fn(&Self) -> bool + Copy) -> bool {
        predicate(self) || self.children.iter().any(|child| child.any(predicate))
    }
}
#[cfg(test)]
pub(super) struct MissionExplain {
    pub tasks: ExplainNode,
    pub summary: String,
}

#[cfg(test)]
pub(super) async fn explain_mission(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    id: &MissionId,
) -> Result<MissionExplain> {
    let mut response = store
        .client()
        .query(include_str!("queries/task_index/explain_mission.surql"))
        .bind(scope(identity)?)
        .bind(("mission", id.to_string()))
        .await?
        .check()?;
    let tasks = ExplainNode::from_value(response.take::<surrealdb::types::Value>(2)?)?;
    let plans = response.take::<surrealdb::types::Value>(3)?;
    let executions = response.take::<surrealdb::types::Value>(4)?;
    Ok(MissionExplain {
        summary: format!("{tasks:?} {plans:?} {executions:?}"),
        tasks,
    })
}

pub(super) async fn missions_page(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    after: Option<&MissionId>,
) -> Result<CollectionPage<String>> {
    let identity = identity.clone();
    let after = after.map(ToString::to_string);
    let runtime = TaskRuntime::new(store.clone(), "uav-sim", "catalog-read");
    veoveo_platform_store::read_transaction::read(store.client(), move |transaction| {
        Box::pin(async move {
            let mut response = transaction
                .query(include_str!("queries/task_index/mission_candidates.surql"))
                .query(include_str!("queries/task_index/missions_page.surql"))
                .bind(scope(&identity)?)
                .bind(("after", after))
                .bind(("limit", index::PAGE_SIZE + 1))
                .await?
                .check()?;
            let rows: Vec<Mission> = response.take(3)?;
            validate_missions(transaction, &runtime, &identity, &rows).await?;
            index::page(
                rows,
                |row| {
                    Ok(UavMissionCursor::new(row.mission_id.clone())?
                        .as_str()
                        .to_owned())
                },
                |row| Ok(uris::mission(&row.mission_id).into()),
            )
        })
    })
    .await
}

#[derive(SurrealValue)]
struct Mission {
    #[surreal(wrap)]
    mission_id: MissionId,
    #[surreal(wrap)]
    plan_ids: Vec<crate::contract::MissionPlanId>,
}

async fn validate_missions<C: surrealdb::Connection>(
    transaction: &surrealdb::method::Transaction<C>,
    runtime: &TaskRuntime,
    identity: &GatewayInternalIdentity,
    missions: &[Mission],
) -> Result<()> {
    if missions.is_empty() {
        return Ok(());
    }
    let plans = missions
        .iter()
        .flat_map(|row| row.plan_ids.iter().map(ToString::to_string))
        .collect::<Vec<_>>();
    let mut response = transaction
        .query(include_str!("queries/task_index/mission_candidates.surql"))
        .query(include_str!("queries/task_index/validate_missions.surql"))
        .bind(scope(identity)?)
        .bind(("plan_ids", plans))
        .await?
        .check()?;
    let rows: Vec<super::task_catalog::CatalogRow> = response.take(3)?;
    let ids = rows
        .iter()
        .map(|row| row.task_id())
        .collect::<Result<Vec<_>>>()?;
    let tasks = runtime
        .for_owner(&runtime_owner(identity))
        .of_types(UavTaskKind::ALL.iter().map(|kind| kind.name()))?
        .get_many_in(transaction, &ids)
        .await?;
    for row in &rows {
        let id = row.task_id()?;
        let task = tasks
            .iter()
            .find(|task| task.task_id == id)
            .ok_or_else(|| anyhow::anyhow!("selected UAV lookup lost its authorized Task"))?;
        row.check(task)?;
    }
    for mission in missions {
        anyhow::ensure!(
            rows.iter().any(|row| row
                .identity
                .plan_id
                .as_ref()
                .is_some_and(|plan| mission.plan_ids.contains(plan))),
            "selected mission has no validated Task lookup"
        );
    }
    Ok(())
}

pub(super) enum CompletionDomain {
    Tasks,
    Missions,
}

pub(super) async fn complete(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    domain: CompletionDomain,
    needle: &str,
) -> Result<Vec<String>> {
    let identity = identity.clone();
    let needle = needle.to_lowercase();
    let runtime = TaskRuntime::new(store.clone(), "uav-sim", "catalog-read");
    veoveo_platform_store::read_transaction::read(store.client(), move |transaction| {
        Box::pin(async move {
            let sql = match domain {
                CompletionDomain::Tasks => include_str!("queries/task_index/complete_tasks.surql"),
                CompletionDomain::Missions => {
                    include_str!("queries/task_index/complete_missions.surql")
                }
            };
            let query = match domain {
                CompletionDomain::Tasks => transaction.query(sql),
                CompletionDomain::Missions => transaction
                    .query(include_str!("queries/task_index/mission_candidates.surql"))
                    .query(sql),
            };
            let mut response = query
                .bind(scope(&identity)?)
                .bind(("needle", needle))
                .bind(("limit", index::PAGE_SIZE + 1))
                .await?
                .check()?;
            match domain {
                CompletionDomain::Tasks => {
                    #[derive(SurrealValue)]
                    struct Completion {
                        value: String,
                    }
                    let rows: Vec<Completion> = response.take(0)?;
                    Ok(rows.into_iter().map(|row| row.value).collect())
                }
                CompletionDomain::Missions => {
                    let rows: Vec<Mission> = response.take(3)?;
                    validate_missions(transaction, &runtime, &identity, &rows).await?;
                    Ok(rows
                        .into_iter()
                        .map(|row| row.mission_id.to_string())
                        .collect())
                }
            }
        })
    })
    .await
}
