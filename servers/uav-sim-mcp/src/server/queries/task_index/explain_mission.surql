LET $matching_plans = (SELECT id, plan_id FROM uav_vehicle_mission_plan WHERE tenant = $tenant AND work_context = $context AND principal_key = $principal_key AND mission_id = $mission);
LET $matching_tasks = array::flatten(array::map($matching_plans, |$matching_plan| {
    LET $plan_record = $matching_plan.id;
    LET $executions = (SELECT * FROM uav_mission_execution WITH INDEX uav_execution_plan WHERE plan = $plan_record);
    RETURN (SELECT VALUE task FROM $executions WHERE plan = $plan_record AND tenant = $tenant AND work_context = $context AND principal_key = $principal_key AND plan.tenant = tenant AND plan.work_context = work_context AND plan.principal_key = principal_key AND plan.state != 'prepared' AND task.request.input.plan_id = plan.plan_id AND record::id(id) = record::id(task));
}));
SELECT * FROM $matching_tasks WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile AND (owner_context.tenant_key ?? NONE) = $tenant_key AND owner_context.data_labels ALLINSIDE $data_labels AND work_context = $context AND task_type = $mission_task_type AND request.input.plan_id IN $matching_plans.plan_id ORDER BY created_at DESC, id DESC LIMIT 1 EXPLAIN FULL;
SELECT id, plan_id FROM uav_vehicle_mission_plan WHERE tenant = $tenant AND work_context = $context AND principal_key = $principal_key AND mission_id = $mission EXPLAIN FULL;
RETURN array::map($matching_plans, |$matching_plan| {
    LET $plan_record = $matching_plan.id;
    RETURN (SELECT * FROM uav_mission_execution WITH INDEX uav_execution_plan WHERE plan = $plan_record EXPLAIN FULL);
});
