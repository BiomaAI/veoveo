-- Materialize each scoped owner association once before caller policy and paging.
LET $executions = SELECT task, plan FROM uav_mission_execution
    WHERE tenant = $tenant AND work_context = $context AND principal_key = $principal_key
        AND plan.tenant = tenant AND plan.work_context = work_context
        AND plan.principal_key = principal_key AND plan.state != 'prepared'
        AND record::id(id) = record::id(task);
LET $lookups = SELECT task, identity.plan_id AS plan_id FROM uav_task
    WHERE identity.tenant = $tenant AND identity.work_context = $context
        AND identity.principal_key = $principal_key AND task_type = $mission_task_type
        AND task IN $executions.task
        AND id = type::record('uav_task', type::string(record::id(task)))
        AND fn::kernel::tasks::selection_v1(task, $server, $tenant, $owner, $profile,
            $principal_key, $profile_key, $tenant_key, $data_labels, $types,
            $context, $context_key, $authority_tenant) != NONE;
LET $admitted_plans = SELECT VALUE plan FROM $executions
    WHERE (SELECT VALUE task FROM $lookups
        WHERE task = $parent.task AND plan_id = $parent.plan.plan_id) != [];
