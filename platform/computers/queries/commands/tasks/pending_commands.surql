SELECT *, payload.sealed AS sealed FROM computer_execution
WHERE provider_instance_id = $provider
  AND ($after = NONE OR execution_id > $after)
  AND {
    LET $tenant_matches = fn::kernel::identity::tenant_matches_v1($enterprise, task_tenant, authority.invocation.tenant);
    LET $state = fn::kernel::tasks::lifecycle_v1(task, $server, task_tenant, ['computer.execution'], NONE);
    LET $missing = !fn::kernel::tasks::exists_v1(task);
    $tenant_matches
      AND ($missing OR $state != NONE)
      AND (stage != 'recovery_required' OR $missing OR $state.status IN ['queued', 'running'])
      AND (task_projected_at = NONE OR (!$missing AND $state.retention_pins CONTAINS string::concat('computer-execution/', <string>execution_id)));
  }
ORDER BY execution_id
LIMIT $limit;
