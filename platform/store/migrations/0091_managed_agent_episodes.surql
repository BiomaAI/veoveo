-- Managed episode admission and explicit stop share the durable runtime transaction.
DEFINE FIELD admission_count ON managed_agent TYPE int DEFAULT 0 ASSERT $value >= 0;
UPDATE managed_agent SET admission_count = 0 WHERE admission_count = NONE;
DEFINE FIELD managed_ready ON agent TYPE option<object> FLEXIBLE;
DEFINE FIELD managed ON agent_episode TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE state ON agent_episode TYPE 'running' | 'completed' | 'budget_terminated' | 'failed' | 'crashed' | 'stopped';

DEFINE FUNCTION fn::agent_consume_results($agent: record<agent>, $wakes: array<record<wake>>, $episode: record<agent_episode>, $now: datetime) -> bool {
LET $deliveries = (SELECT task_id, retention_pin FROM agent_task WHERE agent = $agent AND result_wake IN $wakes AND consumed_by_episode = NONE AND retention_pin_active = true);
FOR $delivery IN $deliveries {
    LET $route = (SELECT * FROM ONLY type::record('gateway_task_route', $delivery.task_id));
    IF $route = NONE { THROW 'gateway task route missing during retention release'; };
    IF $route.source_task != NONE {
        LET $source_task = (SELECT * FROM ONLY $route.source_task);
        IF $source_task = NONE { THROW 'canonical task missing during retention release'; };
        IF $source_task.server != $route.server { THROW 'canonical task server changed during retention release'; };
        IF !($source_task.retention_pins CONTAINS $delivery.retention_pin) { THROW 'canonical task retention pin missing during release'; };
        LET $released = (UPDATE ONLY $route.source_task SET retention_pins -= $delivery.retention_pin WHERE server = $route.server AND retention_pins CONTAINS $delivery.retention_pin RETURN AFTER);
        IF $released = NONE { THROW 'canonical task retention release failed'; };
    };
};
UPDATE agent_task SET consumed_by_episode = $episode, retention_pin_active = false, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now, revision += 1 WHERE agent = $agent AND result_wake IN $wakes AND consumed_by_episode = NONE RETURN NONE;
RETURN true;
};
