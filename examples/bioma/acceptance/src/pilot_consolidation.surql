-- Coordinated installation cutover; all four workloads must already be absent.
BEGIN TRANSACTION;
FOR $entry IN $entries {
    LET $before = $entry.before;
    LET $after = $entry.after;
    LET $current = SELECT * OMIT admission_count FROM ONLY $before.id;
    IF $current = $after { CONTINUE; };
    IF $current != $before { THROW 'pilot changed after consolidation was prepared'; };
    LET $runtime = array::first(SELECT * FROM agent WHERE tenant = $before.tenant AND agent_key = $before.key LIMIT 1);
    IF $runtime = NONE OR $runtime.lease_expires_at > time::now()
        OR $runtime.last_episode.state = 'running' OR $runtime.state = 'running' {
        THROW 'pilot writer has not drained';
    };
    LET $old = SELECT * FROM ONLY $before.definition;
    LET $definition = SELECT * FROM ONLY $after.definition;
    LET $revision = SELECT * FROM ONLY $after.requested_revision;
    IF !$old.disabled OR $definition = NONE OR $definition.disabled OR $definition.status != 'enabled'
        OR $definition.tenant != $before.tenant OR $before.work_context NOT IN $definition.audience
        OR $definition.published != $revision.id OR $revision.definition != $definition.id
        OR $revision.content.execution.kind != 'managed' OR $revision.content.execution.template != 'uav-pilot'
        OR $revision.content.execution.parameters.vehicle != NONE {
        THROW 'pilot definitions changed or target publication is invalid';
    };
    UPDATE ONLY $before.operation SET phase = 'superseded', lease_owner = NONE,
        lease_expires_at = NONE, updated_at = $after.updated_at;
    -- Preserve admission counters and any store-owned bookkeeping.
    UPDATE ONLY $before.id SET definition = $after.definition,
        requested_revision = $after.requested_revision, active_revision = NONE,
        generation = $after.generation, dispatch_epoch = $after.dispatch_epoch,
        resources.template_config_map = $after.resources.template_config_map,
        operation = $after.operation, updated_at = $after.updated_at;
    CREATE ONLY $entry.operation.id CONTENT $entry.operation;
    CREATE outbox_event SET tenant = $before.tenant, aggregate_type = 'managed_agent', aggregate_id = $before.key,
        event_type = 'managed_agent.consolidate', schema_version = 1,
        payload = {key: $before.key, generation: $after.generation, work_context: $before.work_context,
            actor: $before.owner, operation: $after.operation, previous_definition: $before.definition,
            definition: $after.definition};
};
RETURN $entries.map(|$entry| $entry.after.id);
COMMIT TRANSACTION;
