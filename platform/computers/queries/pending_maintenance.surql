SELECT * FROM computer_maintenance
WHERE provider_instance_id = $provider
  AND ($after = NONE OR operation_id > $after)
  AND {
    LET $tenant_matches = fn::kernel::identity::tenant_matches_v1($enterprise, task_tenant, (actor_context.tenant_key ?? 'installation'));
    LET $state = fn::kernel::tasks::lifecycle_v1(task, $server, task_tenant, ['computer.maintenance'], NONE);
    LET $missing = !fn::kernel::tasks::exists_v1(task);
    $tenant_matches
      AND (actor_context.authority.tenant = (actor_context.tenant_key ?? 'installation'))
      AND ($missing OR $state != NONE)
      AND (stage != 'recovery_required' OR $missing OR $state.status IN ['queued', 'running'])
      AND (task_projected_at = NONE OR (!$missing AND $state.retention_pins CONTAINS string::concat('computer-maintenance/', <string>operation_id)));
  }
ORDER BY operation_id
LIMIT $limit;
