SELECT VALUE task FROM uav_task WHERE task_type = $task_type
    AND identity.plan_id != NONE AND created_at <= $before
    AND ($after = NONE OR task > $after)
    AND id = type::record('uav_task', type::string(record::id(task)))
    AND (SELECT VALUE id FROM uav_vehicle_mission_plan
        WHERE tenant = $parent.identity.tenant AND work_context = $parent.identity.work_context
        AND principal_key = $parent.identity.principal_key AND plan_id = $parent.identity.plan_id
        AND state IN ['prepared', 'completed', 'failed', 'cancelled']) != []
    AND { LET $lifecycle = fn::kernel::tasks::lifecycle_v1(task, $server, identity.tenant, [$task_type], NONE);
        type::is_object($lifecycle) AND $lifecycle.work_context = identity.work_context
            AND $lifecycle.owner = identity.owner AND $lifecycle.profile = identity.profile
            AND $lifecycle.recovery_class = 'interrupted_indeterminate'
            AND $lifecycle.retention_pins CONTAINS $pin
            AND $lifecycle.status IN ['succeeded', 'failed', 'cancelled']; }
    ORDER BY task ASC LIMIT 100;
