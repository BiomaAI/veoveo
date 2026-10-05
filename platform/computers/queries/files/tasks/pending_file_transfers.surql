SELECT *, payload.sealed AS sealed FROM computer_file_transfer WHERE provider_instance_id = $provider
             AND ($after = NONE OR transfer_id > $after)
             AND (stage != 'recovery_required' OR task.status = NONE OR task.status IN ['queued', 'running'])
             AND (task_projected_at = NONE OR task.retention_pins CONTAINS string::concat('computer-file-transfer/', <string>transfer_id))
             ORDER BY transfer_id LIMIT $limit;
