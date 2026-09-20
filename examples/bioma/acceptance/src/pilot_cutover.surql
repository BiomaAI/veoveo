-- Installation-only, exact before-image adoption. Never a public API.
BEGIN TRANSACTION;
FOR $entry IN $entries {
    LET $instance = $entry.instance;
    LET $current = SELECT * FROM ONLY $instance.id;
    IF $current != NONE {
        IF $current.generation != 1 OR $current.desired != 'paused'
            OR $current.principal != $instance.principal OR $current.identity != $instance.identity
            OR $current.resources != $instance.resources OR $current.public_key != $instance.public_key
            OR $current.definition != $instance.definition OR $current.requested_revision != $instance.requested_revision
            OR $current.operation != $instance.operation { THROW 'pilot adoption conflicts with existing lifecycle ownership'; };
        CONTINUE;
    };
    LET $runtime = SELECT * FROM ONLY $entry.runtime.id;
    LET $principal = SELECT * FROM ONLY $entry.principal.id;
    IF $runtime != $entry.runtime OR $principal != $entry.principal
        OR $runtime.lease_expires_at > time::now() OR $runtime.last_episode.state = 'running'
        OR $runtime.state = 'running' { THROW 'pilot source changed or its writer has not drained'; };
    LET $definition = SELECT * FROM ONLY $instance.definition;
    LET $revision = SELECT * FROM ONLY $instance.requested_revision;
    IF $definition = NONE OR $definition.key != $instance.key OR $definition.tenant != $instance.tenant
        OR $definition.status != 'enabled' OR $definition.published != $revision.id
        OR $instance.work_context NOT IN $definition.audience OR $revision.content.execution.kind != 'managed'
        OR $revision.content.execution.template != 'uav-pilot' { THROW 'published pilot definition changed'; };
    LET $capacity = type::record('managed_agent_capacity', $instance.work_context.id);
    UPSERT ONLY $capacity SET tenant = $instance.tenant, work_context = $instance.work_context,
        instances = (instances ?? 0) + 1, storage_gib = (storage_gib ?? 0) + $instance.resources.storage_gib;
    IF $capacity.instances > 32 OR $capacity.storage_gib > 128 { THROW 'pilot adoption exceeds Bioma capacity'; };
    CREATE ONLY $instance.id CONTENT $instance;
    CREATE ONLY $entry.operation.id CONTENT $entry.operation;
    CREATE outbox_event SET tenant = $instance.tenant, aggregate_type = 'managed_agent', aggregate_id = $instance.key,
        event_type = 'managed_agent.adopt', schema_version = 1,
        payload = {key: $instance.key, generation: 1, work_context: $instance.work_context,
            actor: $instance.owner, operation: $instance.operation, runtime: $runtime.id};
};
RETURN $entries.map(|$entry| $entry.instance.id);
COMMIT TRANSACTION;
