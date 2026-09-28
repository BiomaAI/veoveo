//! Authorized task and mission indexes; SQL performs every persisted-row selection.
use anyhow::Result;
use surrealdb::types::SurrealValue;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{
    PlatformStore, RecordId, TaskRecord, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id,
};
use veoveo_task_runtime::{TaskPageCursor, TaskRuntime, TaskSnapshot};
use veoveo_types::TaskId;

use super::{index, ownership::runtime_owner};
use crate::{
    contract::{CollectionPage, MissionId, UavMissionCursor, UavUsageCursor, UavUsagePosition},
    uris,
};

const TASK_TYPES: &[&str] = &[
    "run_scenario",
    "capture_dataset",
    "execute_vehicle_mission_plan",
];
const VISIBLE: &str = "server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND (request.owner.tenant_key ?? NONE) = $tenant_key AND request.owner.data_labels ALLINSIDE $data_labels";
const PLAN_VISIBLE: &str =
    "tenant = $tenant AND work_context = $context AND principal_key = $principal_key";
const MISSION_TASK: &str = "work_context = $context AND task_type = 'execute_vehicle_mission_plan'";
const EXECUTION_LINK: &str = "tenant = $tenant AND work_context = $context
    AND principal_key = $principal_key
    AND plan.tenant = tenant AND plan.work_context = work_context
    AND plan.principal_key = principal_key AND plan.state != 'prepared'
    AND task.request.input.plan_id = plan.plan_id
    AND record::id(id) = record::id(task)";

fn admitted_plans_sql() -> String {
    format!(
        "SELECT VALUE plan FROM uav_mission_execution WHERE {EXECUTION_LINK}
         AND task IN (SELECT VALUE id FROM task WHERE {VISIBLE} AND {MISSION_TASK})"
    )
}

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
        .of_types(
            TASK_TYPES
                .iter()
                .copied()
                .map(veoveo_types::TaskTypeName::from_static),
        )?
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
        .query(format!(
            "SELECT * FROM ONLY $task WHERE {VISIBLE} AND task_type IN $types;"
        ))
        .bind(("task", veoveo_platform_store::task_record_id(id)))
        .bind(scope(identity)?)
        .bind(("types", TASK_TYPES.to_vec()))
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
        .query(mission_sql())
        .bind(scope(identity)?)
        .bind(("mission", id.to_string()))
        .await?
        .check()?;
    let rows: Vec<TaskRecord> = response.take(1)?;
    rows.into_iter()
        .next()
        .map(TaskSnapshot::try_from)
        .transpose()
        .map_err(Into::into)
}

fn mission_sql() -> String {
    // Resolve matching plans before planning the Task read. Select their index explicitly:
    // the creation-order owner index can scan unrelated history for an old mission.
    format!(
        "LET $matching_plans = ({});
         SELECT * FROM task WITH INDEX task_uav_plan WHERE {VISIBLE} AND {MISSION_TASK}
         AND request.input.plan_id IN $matching_plans.plan_id
         AND id IN (SELECT VALUE task FROM uav_mission_execution
             WHERE plan IN $matching_plans.id AND {EXECUTION_LINK})
         ORDER BY created_at DESC, id DESC LIMIT 1",
        mission_plan_sql()
    )
}

fn mission_plan_sql() -> String {
    format!(
        "SELECT id, plan_id FROM uav_vehicle_mission_plan WHERE {PLAN_VISIBLE} AND mission_id = $mission"
    )
}

#[cfg(test)]
pub(super) async fn explain_mission(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    id: &MissionId,
) -> Result<String> {
    let mut response = store
        .client()
        .query(format!(
            "{} EXPLAIN FULL; {} EXPLAIN FULL",
            mission_sql(),
            mission_plan_sql()
        ))
        .bind(scope(identity)?)
        .bind(("mission", id.to_string()))
        .await?
        .check()?;
    Ok(format!(
        "{:?} {:?}",
        response.take::<surrealdb::types::Value>(1)?,
        response.take::<surrealdb::types::Value>(2)?
    ))
}

pub(super) async fn missions_page(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    after: Option<&MissionId>,
) -> Result<CollectionPage<String>> {
    let mut response = store
        .client()
        .query(format!(
            "SELECT mission_id FROM uav_vehicle_mission_plan WHERE {PLAN_VISIBLE}
         AND id IN ({})
         AND ($after = NONE OR mission_id > $after)
         GROUP BY mission_id ORDER BY mission_id ASC LIMIT $limit;",
            admitted_plans_sql()
        ))
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
                UavMissionCursor::new(MissionId::new(row.mission_id.clone())?)?
                    .as_str()
                    .to_owned(),
            )
        },
        |row| Ok(uris::mission(&MissionId::new(row.mission_id)?).into()),
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
        CompletionDomain::Tasks => format!(
            "SELECT type::string(record::id(id)) AS value FROM task WHERE {VISIBLE} AND task_type IN $types
             AND string::contains(string::lowercase(type::string(record::id(id))), $needle) ORDER BY value ASC LIMIT $limit;"
        ),
        CompletionDomain::Missions => format!(
            "SELECT mission_id AS value FROM uav_vehicle_mission_plan WHERE {PLAN_VISIBLE}
             AND id IN ({})
             AND string::contains(string::lowercase(mission_id), $needle) GROUP BY value ORDER BY value ASC LIMIT $limit;", admitted_plans_sql()
        ),
    };
    #[derive(SurrealValue)]
    struct Completion {
        value: String,
    }
    let mut response = store
        .client()
        .query(sql)
        .bind(scope(identity)?)
        .bind(("types", TASK_TYPES.to_vec()))
        .bind(("needle", needle.to_lowercase()))
        .bind(("limit", index::PAGE_SIZE + 1))
        .await?
        .check()?;
    let rows: Vec<Completion> = response.take(0)?;
    Ok(rows.into_iter().map(|row| row.value).collect())
}
