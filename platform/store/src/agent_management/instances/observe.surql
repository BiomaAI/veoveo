IF !fn::managed_agent_claim($command.claim) { THROW 'agent_conflict'; };
LET $instance = SELECT * FROM ONLY $command.claim.instance;
LET $operation = SELECT * FROM ONLY $command.claim.operation;
LET $phase = $command.phase;
LET $previous = $operation.phase;
IF $phase != 'failed' AND $phase != $previous AND !(
    ($previous = 'queued' AND $phase = 'credentials') OR
    ($previous = 'credentials' AND $phase = 'storage') OR
    ($previous = 'storage' AND $phase = 'draining') OR
    ($previous = 'draining' AND $phase IN ['workload', 'paused', 'archived']) OR
    ($previous = 'workload' AND $phase = 'ready') OR
    ($previous = 'ready' AND $phase = 'workload')
) { THROW 'agent_conflict'; };
IF ($phase = 'paused' AND $instance.desired != 'paused')
    OR ($phase = 'archived' AND $instance.desired != 'archived')
    OR ($phase IN ['workload', 'ready'] AND $instance.desired != 'running') { THROW 'agent_conflict'; };
IF $phase IN ['workload', 'archived'] AND $phase != $previous {
    LET $runtime = array::first(SELECT * FROM agent WHERE tenant = $instance.tenant AND agent_key = $instance.key LIMIT 1);
    IF $runtime != NONE AND $runtime.lease_expires_at > time::now() { THROW 'agent_conflict'; };
};
IF $phase = 'paused' {
    LET $runtime = array::first(SELECT * FROM agent WHERE tenant = $instance.tenant AND agent_key = $instance.key LIMIT 1);
    IF $runtime != NONE AND $runtime.lease_expires_at > time::now() AND $runtime.last_episode.state = 'running' { THROW 'agent_conflict'; };
};
IF $phase IN ['workload', 'ready'] {
    IF !fn::managed_agent_enabled($instance.id) OR $instance.public_key = NONE { THROW 'agent_forbidden'; };
};
IF $phase = 'workload' {
    UPDATE ONLY $instance.id SET active_revision = requested_revision, active_generation = generation;
};
UPDATE ONLY $instance.id SET observed = $phase, updated_at = time::now();
LET $result = UPDATE ONLY $operation.id SET phase = $phase, message = $command.message, updated_at = time::now(),
    lease_owner = IF $phase IN ['ready', 'paused', 'archived', 'failed'] { NONE } ELSE { lease_owner },
    lease_expires_at = IF $phase IN ['ready', 'paused', 'archived', 'failed'] { NONE } ELSE { lease_expires_at };
CREATE outbox_event SET tenant = $instance.tenant, aggregate_type = 'managed_agent', aggregate_id = $instance.key,
    event_type = 'managed_agent.observed', schema_version = 1,
    payload = { key: $instance.key, generation: $instance.generation, phase: $phase,
        work_context: $instance.work_context, operation: $operation.id };
RETURN $result;
