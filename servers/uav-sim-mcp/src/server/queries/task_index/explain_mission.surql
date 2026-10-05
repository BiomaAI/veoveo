LET $matching_plans = (SELECT id, plan_id FROM uav_vehicle_mission_plan WHERE tenant = $tenant AND work_context = $context AND principal_key = $principal_key AND mission_id = $mission);
LET $matching_tasks = array::flatten(array::map($matching_plans, |$matching_plan| {
    LET $plan_record = $matching_plan.id;
    LET $executions = (SELECT * FROM uav_mission_execution WITH INDEX uav_execution_plan WHERE plan = $plan_record AND tenant = $tenant AND work_context = $context AND principal_key = $principal_key AND plan.tenant = tenant AND plan.work_context = work_context AND plan.principal_key = principal_key AND plan.state != 'prepared' AND record::id(id) = record::id(task));
    RETURN (SELECT * FROM uav_task WHERE task IN $executions.task AND identity.plan_id = $matching_plan.plan_id);
}));
SELECT * FROM $matching_tasks WHERE id = type::record('uav_task', type::string(record::id(task))) AND identity.tenant = $tenant AND identity.work_context = $context AND identity.principal_key = $principal_key AND fn::kernel::tasks::selection_v1(task, $server, $tenant, $owner, $profile, $principal_key, $profile_key, $tenant_key, $data_labels, $types, $context, $context_key, $authority_tenant) != NONE AND task_type = $mission_task_type ORDER BY created_at DESC, task DESC LIMIT 1 EXPLAIN FULL;
SELECT id, plan_id FROM uav_vehicle_mission_plan WHERE tenant = $tenant AND work_context = $context AND principal_key = $principal_key AND mission_id = $mission EXPLAIN FULL;
RETURN array::map($matching_plans, |$matching_plan| { LET $plan_record = $matching_plan.id; RETURN (SELECT * FROM uav_mission_execution WITH INDEX uav_execution_plan WHERE plan = $plan_record EXPLAIN FULL); });
