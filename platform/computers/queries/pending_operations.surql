SELECT * FROM computer_operation
WHERE provider_instance_id = $provider
  AND ($after = NONE OR operation_id > $after)
  AND (stage != 'recovery_required' OR task.status = NONE OR task.status IN ['queued', 'running'])
  AND (task_projected_at = NONE OR task.retention_pins CONTAINS string::concat('computer-operation/', <string>operation_id))
ORDER BY operation_id
LIMIT $limit;
