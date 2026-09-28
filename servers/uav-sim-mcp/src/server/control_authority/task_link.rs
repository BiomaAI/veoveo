//! Native Task identity and retention through mission settlement.
use surrealdb::types::RecordId;
use veoveo_task_runtime::TaskRetentionPin;
use veoveo_types::TaskId;
use veoveo_types::TaskTypeDefinition;

pub(in crate::server) fn retention_pin() -> TaskRetentionPin {
    TaskRetentionPin::new("uav-sim:mission-execution").expect("static mission retention pin")
}

pub(super) fn record(task: TaskId) -> RecordId {
    RecordId::new(
        "uav_mission_execution",
        surrealdb::types::Uuid::from(task.as_uuid()),
    )
}

impl super::VehicleControlAuthority {
    pub(in crate::server) async fn settled_task_ids(
        &self,
        after: Option<TaskId>,
        before: chrono::DateTime<chrono::Utc>,
    ) -> super::Result<Vec<TaskId>> {
        // SQL selects releasable candidates before the page bound. Exact reads below
        // validate the selected plan's canonical document before acknowledging its pin.
        let mut response = self
            .store
            .client()
            .query(
                "SELECT VALUE id FROM task
             WHERE server = $server AND task_type = $task_type
             AND recovery_class = 'interrupted_indeterminate'
             AND retention_pins CONTAINS $pin AND status IN ['succeeded', 'failed', 'cancelled']
             AND created_at <= $before AND ($after = NONE OR id > $after)
             AND (SELECT VALUE id FROM uav_vehicle_mission_plan
                 WHERE tenant = $parent.tenant AND work_context = $parent.work_context
                 AND principal_key = $parent.request.owner.principal_key
                 AND plan_id = $parent.request.input.plan_id
                 AND state IN ['prepared', 'completed', 'failed', 'cancelled'] LIMIT 1) != []
             ORDER BY id ASC LIMIT 100;",
            )
            .bind(("server", RecordId::new("mcp_server", "uav-sim")))
            .bind((
                "task_type",
                crate::contract::UavTaskKind::ExecuteMission
                    .name()
                    .to_string(),
            ))
            .bind(("pin", retention_pin().to_string()))
            .bind(("before", before))
            .bind(("after", after.map(veoveo_platform_store::task_record_id)))
            .await?
            .check()?;
        response
            .take::<Vec<RecordId>>(0)?
            .into_iter()
            .map(|id| {
                if let surrealdb::types::RecordIdKey::Uuid(uuid) = id.key {
                    Ok(TaskId::from_uuid(uuid.into()))
                } else {
                    Err(super::ControlAuthorityError::Invalid(
                        "retained mission Task has a non-UUID key".into(),
                    ))
                }
            })
            .collect()
    }

    pub(in crate::server) async fn task_retention_releasable(
        &self,
        task: &veoveo_task_runtime::TaskSnapshot,
    ) -> super::Result<bool> {
        use super::*;
        use crate::contract::ExecuteVehicleMissionPlanRequest;
        if task.server != "uav-sim"
            || task.task_type != crate::contract::UavTaskKind::ExecuteMission.name()
            || task.recovery_class != veoveo_task_runtime::RecoveryClass::InterruptedIndeterminate
            || !task.is_terminal()
        {
            return Ok(false);
        }
        let request: ExecuteVehicleMissionPlanRequest =
            serde_json::from_value(task.request.clone())?;
        let tenant = &task.owner.authority.tenant;
        let context = &task.owner.authority.work_context;
        let plan_id = scoped_context_record_id(
            "uav_vehicle_mission_plan",
            tenant,
            context,
            request.plan_id.as_str(),
        );
        let mut response = self.store.client().query(
            "SELECT * FROM ONLY $plan WHERE tenant = $tenant AND work_context = $context AND principal_key = $principal;
             SELECT VALUE (plan = $plan AND task = $task AND tenant = $tenant AND work_context = $context AND principal_key = $principal) FROM ONLY $execution;"
        ).bind(("plan", plan_id)).bind(("tenant", deterministic_tenant_id(tenant.as_str())?.record_id()))
            .bind(("context", deterministic_work_context_id(tenant.as_str(), context.as_str())?.record_id()))
            .bind(("principal", task.owner.principal_key.clone()))
            .bind(("task", veoveo_platform_store::task_record_id(task.task_id)))
            .bind(("execution", record(task.task_id)))
            .await?.check()?;
        let Some(row) = response.take::<Option<PlanRecord>>(0)? else {
            return Ok(false);
        };
        let plan = plan_view(&row)?;
        // An existing mismatched link is corruption, never proof that admission did not occur.
        let linked = match response.take::<Option<bool>>(1)? {
            Some(true) => true,
            Some(false) => return Err(ControlAuthorityError::Conflict),
            None => false,
        };
        match plan.state {
            MissionPlanLifecycle::Executing => Ok(false),
            MissionPlanLifecycle::Prepared => Ok(!linked),
            MissionPlanLifecycle::Completed
            | MissionPlanLifecycle::Failed
            | MissionPlanLifecycle::Cancelled => Ok(true),
        }
    }
}
