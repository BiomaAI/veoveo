SELECT VALUE id FROM task
             WHERE server = $server AND task_type = $task_type
             AND recovery_class = 'interrupted_indeterminate'
             AND retention_pins CONTAINS $pin AND status IN ['succeeded', 'failed', 'cancelled']
             AND created_at <= $before AND ($after = NONE OR id > $after)
             AND (SELECT VALUE id FROM uav_vehicle_mission_plan
                 WHERE tenant = $parent.tenant AND work_context = $parent.work_context
                 AND principal_key = $parent.owner_context.principal_key
                 AND plan_id = $parent.request.input.plan_id
                 AND state IN ['prepared', 'completed', 'failed', 'cancelled'] LIMIT 1) != []
             ORDER BY id ASC LIMIT 100;
