LET $now = time::now();
LET $waiting = SELECT tenant, key FROM managed_agent
    WHERE resources.namespace = $command AND observed NOT IN ['archived', 'failed', 'paused']
      AND (observed = 'draining' OR desired != 'running');
LET $identities = $waiting.map(|$row| [$row.tenant, $row.key]);
RETURN {
    now: $now,
    claim: array::first(SELECT VALUE lease_expires_at FROM managed_agent_operation
        WHERE instance.resources.namespace = $command AND instance.operation = id
          AND phase NOT IN ['archived', 'failed', 'superseded'] AND lease_expires_at > $now
        ORDER BY lease_expires_at LIMIT 1),
    startup: array::first((SELECT updated_at + 10m AS due FROM managed_agent
        WHERE resources.namespace = $command AND observed = 'workload'
          AND (operation.lease_expires_at = NONE OR operation.lease_expires_at <= $now)
        ORDER BY due LIMIT 1)).due,
    drain: array::first(SELECT VALUE lease_expires_at FROM agent
        WHERE lease_expires_at > $now AND [tenant, agent_key] IN $identities
        ORDER BY lease_expires_at LIMIT 1)
};
