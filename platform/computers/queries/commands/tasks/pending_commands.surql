SELECT *, payload.sealed AS sealed FROM computer_execution WHERE provider_instance_id = $provider
             AND ($after = NONE OR execution_id > $after)
             AND (stage != 'recovery_required' OR task.status = NONE OR task.status IN ['queued', 'running'])
             AND (task_projected_at = NONE OR task.retention_pins CONTAINS string::concat('computer-execution/', <string>execution_id))
             ORDER BY execution_id LIMIT $limit;
