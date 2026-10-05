//! Authorized task and mission indexes; SQL performs every persisted-row selection.
use crate::contract::UavTaskKind;
use anyhow::Result;
use surrealdb::types::SurrealValue;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{
    PlatformStore, RecordId, TaskRecord, deterministic_principal_id, deterministic_tenant_id,
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
    tenant_key: Option<String>,
    principal_key: String,
    data_labels: Vec<String>,
    mission_task_type: String,
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
        tenant_key: owner.tenant_key,
        principal_key: owner.principal_key,
        data_labels: owner.data_labels.into_iter().collect(),
        mission_task_type: UavTaskKind::ExecuteMission.name().to_string(),
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
    let mut response = store
        .client()
        .query(include_str!("queries/task_index/task.surql"))
        .bind(("task", veoveo_platform_store::task_record_id(id)))
        .bind(scope(identity)?)
        .bind((
            "types",
            UavTaskKind::ALL
                .iter()
                .map(|kind| kind.name().to_string())
                .collect::<Vec<_>>(),
        ))
        .await?
        .check()?;
    response
        .take::<Option<TaskRecord>>(0)?
        .map(TaskSnapshot::try_from)
        .transpose()
        .map_err(Into::into)
}

pub(super) async fn mission(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    id: &MissionId,
) -> Result<Option<TaskSnapshot>> {
    let mut response = store
        .client()
        .query(include_str!("queries/task_index/mission.surql"))
        .bind(scope(identity)?)
        .bind(("mission", id.to_string()))
        .await?
        .check()?;
    let rows: Vec<TaskRecord> = response.take(2)?;
    rows.into_iter()
        .next()
        .map(TaskSnapshot::try_from)
        .transpose()
        .map_err(Into::into)
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
    let mut response = store
        .client()
        .query(include_str!("queries/task_index/missions_page.surql"))
        .bind(scope(identity)?)
        .bind(("after", after.map(ToString::to_string)))
        .bind(("limit", index::PAGE_SIZE + 1))
        .await?
        .check()?;
    let rows: Vec<Mission> = response.take(0)?;
    index::page(
        rows,
        |row| {
            Ok(
                UavMissionCursor::new(MissionId::parse(row.mission_id.clone())?)?
                    .as_str()
                    .to_owned(),
            )
        },
        |row| Ok(uris::mission(&MissionId::parse(row.mission_id)?).into()),
    )
}

#[derive(SurrealValue)]
struct Mission {
    mission_id: String,
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
    let sql = match domain {
        CompletionDomain::Tasks => include_str!("queries/task_index/complete_tasks.surql"),
        CompletionDomain::Missions => include_str!("queries/task_index/complete_missions.surql"),
    };
    #[derive(SurrealValue)]
    struct Completion {
        value: String,
    }
    let mut response = store
        .client()
        .query(sql)
        .bind(scope(identity)?)
        .bind((
            "types",
            UavTaskKind::ALL
                .iter()
                .map(|kind| kind.name().to_string())
                .collect::<Vec<_>>(),
        ))
        .bind(("needle", needle.to_lowercase()))
        .bind(("limit", index::PAGE_SIZE + 1))
        .await?
        .check()?;
    let rows: Vec<Completion> = response.take(0)?;
    Ok(rows.into_iter().map(|row| row.value).collect())
}
