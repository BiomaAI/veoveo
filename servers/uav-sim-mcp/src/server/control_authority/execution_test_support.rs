//! Native admission fixtures create real retained Tasks before domain mutation.
use super::*;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskRuntime, TaskSnapshot};
use veoveo_types::TaskId;

pub(super) async fn task(
    authority: &VehicleControlAuthority,
    identity: &GatewayInternalIdentity,
    plan: &VehicleMissionPlan,
) -> (TaskRuntime, TaskSnapshot) {
    let tasks = TaskRuntime::new(authority.store.clone(), "uav-sim", "native-admission");
    let task =
        tasks
            .create(CreateTask {
                task_id: TaskId::new(),
                owner: super::super::ownership::runtime_owner(identity),
                server: "uav-sim".into(),
                task_type: const {
                    veoveo_types::TaskTypeName::from_static("execute_vehicle_mission_plan")
                },
                request: serde_json::to_value(crate::contract::ExecuteVehicleMissionPlanRequest {
                    plan_id: plan.plan_id.clone(),
                    expected_revision: plan.revision,
                })
                .unwrap(),
                recovery_class: RecoveryClass::InterruptedIndeterminate,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: None,
                retention_pins: BTreeSet::from([task_link::retention_pin()]),
            })
            .await
            .unwrap()
            .snapshot;
    (tasks, task)
}

pub(super) async fn begin(
    authority: &VehicleControlAuthority,
    identity: &GatewayInternalIdentity,
    plan: &MissionPlanId,
    revision: u64,
) -> Result<(VehicleMissionPlan, MissionExecutionGuard)> {
    let draft = authority
        .prepare_execution(identity, plan, revision)
        .await?;
    let (tasks, task) = task(authority, identity, draft.plan()).await;
    authority.admit_execution(draft, &tasks, &task).await
}
