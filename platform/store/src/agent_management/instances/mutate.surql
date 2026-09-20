IF $authority.membership = 'viewer' { THROW 'agent_forbidden'; };
LET $instance = SELECT * FROM ONLY $command.instance;
IF $instance != NONE AND !fn::managed_agent_editor($authority, $instance) { THROW 'agent_not_found'; };
LET $receipt = SELECT * FROM ONLY $command.operation;
IF $receipt != NONE {
    IF $receipt.fingerprint != $command.fingerprint OR $receipt.work_context != $authority.work_context { THROW 'agent_conflict'; };
    RETURN $receipt;
};
LET $mutation = $command.mutation;
IF $mutation.kind = 'provision' {
    IF $instance != NONE { THROW 'agent_conflict'; };
    LET $plan = $mutation.plan;
    LET $definition = SELECT * FROM ONLY $command.definition;
    LET $revision = SELECT * FROM ONLY $definition.published;
    IF $definition = NONE OR $definition.tenant != $authority.tenant OR $definition.disabled
        OR $definition.status = 'archived' OR $authority.work_context NOT IN $definition.audience
        OR $revision.digest != $plan.revision OR $revision.content.execution.kind != 'managed' { THROW 'agent_not_found'; };
    IF record::exists($command.principal)
        OR array::len(SELECT id FROM agent WHERE tenant = $authority.tenant AND agent_key = $command.key LIMIT 1) > 0
        OR array::len(SELECT id FROM managed_agent WHERE identity.client_id = $plan.identity.client_id LIMIT 1) > 0
        OR array::len(SELECT id FROM oauth_client WHERE client_id = $plan.identity.client_id LIMIT 1) > 0 { THROW 'agent_conflict'; };
    IF array::len(SELECT id FROM managed_agent WHERE resources.namespace = $plan.resources.namespace AND (
        resources.workload = $plan.resources.workload OR resources.credential_secret = $plan.resources.credential_secret
        OR resources.volume_claim = $plan.resources.volume_claim) LIMIT 1) > 0 { THROW 'agent_conflict'; };
    LET $capacity = type::record('managed_agent_capacity', $authority.work_context.id);
    UPSERT ONLY $capacity SET tenant = $authority.tenant, work_context = $authority.work_context,
        instances = (instances ?? 0) + 1, storage_gib = (storage_gib ?? 0) + $plan.resources.storage_gib;
    IF $capacity.instances > $command.limits.instances OR $capacity.storage_gib > $command.limits.storage_gib { THROW 'agent_capacity'; };
    CREATE ONLY $command.principal SET tenant = $authority.tenant, kind = 'service', issuer = $plan.identity.issuer,
        subject = $plan.identity.client_id, display_name = $plan.name, claims_hash = '', enabled = true;
    CREATE ONLY $command.instance SET tenant = $authority.tenant, work_context = $authority.work_context,
        owner = $authority.principal, deployed_by = $authority.principal, key = $command.key, name = $plan.name,
        definition = $command.definition, requested_revision = $revision.id, active_generation = 0,
        generation = 1, dispatch_epoch = 1, desired = 'running', observed = 'queued', principal = $command.principal,
        identity = $plan.identity, resources = $plan.resources, operation = $command.operation;
} ELSE {
    IF $instance = NONE { THROW 'agent_not_found'; };
    IF $instance.generation != $command.expected_generation OR $instance.desired = 'archived' { THROW 'agent_conflict'; };
    IF $mutation.kind = 'state' {
        IF $mutation.desired = 'running' AND !fn::managed_agent_enabled($instance.id) { THROW 'agent_forbidden'; };
        UPDATE ONLY $command.instance SET desired = $mutation.desired,
            dispatch_epoch += IF $mutation.desired = 'archived' { 1 } ELSE { 0 };
    } ELSE IF $mutation.kind = 'revision' {
        IF !fn::managed_agent_enabled($instance.id) { THROW 'agent_forbidden'; };
        LET $revision = array::first(SELECT * FROM agent_definition_revision WHERE definition = $instance.definition AND digest = $mutation.digest LIMIT 1);
        LET $old = SELECT * FROM ONLY $instance.requested_revision;
        IF $revision = NONE OR $revision.content.execution.kind != 'managed'
            OR $revision.content.execution.template != $old.content.execution.template
            OR $revision.content.execution.parameters != $old.content.execution.parameters { THROW 'agent_conflict'; };
        IF $revision.content.execution.template_revision = $old.content.execution.template_revision
            AND $mutation.image != $instance.resources.image { THROW 'agent_conflict'; };
        UPDATE ONLY $command.instance SET requested_revision = $revision.id, resources.image = $mutation.image;
    } ELSE IF $mutation.kind = 'stop' {
        UPDATE ONLY $command.instance SET dispatch_epoch += 1;
        LET $runtime = array::first(SELECT * FROM agent WHERE tenant = $instance.tenant AND agent_key = $instance.key LIMIT 1);
        LET $episode = SELECT * FROM ONLY $runtime.last_episode;
        IF $runtime != NONE AND $episode != NONE AND $episode.state = 'running' AND $episode.managed.instance = $instance.id {
            LET $now = time::now();
            LET $wakes = SELECT VALUE id FROM wake WHERE agent = $runtime.id AND state = 'claimed' AND claimed_by = $runtime.lease_owner AND claim_fence = $runtime.fence;
            fn::agent_consume_results($runtime.id, $wakes, $episode.id, $now);
            UPDATE ONLY $episode.id SET state = 'stopped', final_output = 'Stopped by an authorized operator.', error = NONE, finished_at = $now, revision += 1;
            UPDATE wake SET state = 'acked', acked_at = $now, acked_by_episode = $episode.id, claimed_by = NONE, claimed_at = NONE, claim_expires_at = NONE, claim_fence = NONE, updated_at = $now, revision += 1 WHERE id IN $wakes;
            CREATE outbox_event SET tenant = $instance.tenant, aggregate_type = 'agent_episode', aggregate_id = type::string(record::id($episode.id)), event_type = 'agent_episode.stopped', schema_version = 1,
                payload = { actor: $authority.principal, request_id: $command.request_id, managed_instance: $instance.id };
        };
    } ELSE IF $mutation.kind = 'retry' {
        IF $instance.observed != 'failed' { THROW 'agent_conflict'; };
    } ELSE { THROW 'agent_conflict'; };
    UPDATE ONLY $instance.operation SET phase = 'superseded', lease_owner = NONE, lease_expires_at = NONE, updated_at = time::now();
    UPDATE ONLY $command.instance SET generation += 1, operation = $command.operation, observed = 'queued', updated_at = time::now();
};
LET $current = SELECT * FROM ONLY $command.instance;
LET $result = CREATE ONLY $command.operation SET instance = $command.instance, tenant = $authority.tenant,
    work_context = $authority.work_context, actor = $authority.principal, request_id = $command.request_id,
    fingerprint = $command.fingerprint, generation = $current.generation, phase = 'queued', lease_fence = 0;
CREATE outbox_event SET tenant = $authority.tenant, aggregate_type = 'managed_agent', aggregate_id = $command.key,
    event_type = string::concat('managed_agent.', $mutation.kind), schema_version = 1,
    payload = { key: $command.key, generation: $current.generation, work_context: $authority.work_context,
        actor: $authority.principal, request_id: $command.request_id, operation: $command.operation };
RETURN $result;
