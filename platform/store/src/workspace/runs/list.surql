IF !fn::workspace_member($authority, $command) { THROW 'workspace_not_found'; };
-- Recovery changes state, never re-dispatches a provider request. A browser may
-- return after every original worker is gone and still recover the outcome.
LET $chat = SELECT * FROM ONLY $command;
LET $active = SELECT * FROM workspace_run WHERE chat = $command AND state IN ['queued', 'running'] LIMIT 64;
FOR $run IN $active {
    LET $agent = SELECT * FROM ONLY $run.agent;
    LET $initiator = SELECT * FROM ONLY $run.initiator;
    LET $member = SELECT id FROM workspace_member WHERE chat = $command AND principal = $run.initiator AND active = true LIMIT 1;
    LET $revoked = $chat.archived OR !$agent.active OR $agent.definition_digest != $run.definition_digest
        OR !$initiator.enabled OR count($member) = 0;
    IF $revoked OR $run.deadline <= time::now() OR $run.lease_until <= time::now() {
        LET $head = UPDATE ONLY $command SET sequence += 1, updated_at = time::now();
        UPDATE ONLY $run.id SET state = 'interrupted', fence = NONE, updated_at = time::now(),
            updated_sequence = $head.sequence, failure = IF $revoked { 'permission_changed' }
                ELSE IF $run.deadline <= time::now() { 'deadline' } ELSE { 'worker_lost' };
        CREATE ONLY type::record('workspace_event', [$command, $head.sequence]) SET
            chat = $command, sequence = $head.sequence, kind = 'run', target = $run.id;
    };
};
RETURN SELECT * FROM workspace_run WHERE chat = $command ORDER BY sequence DESC LIMIT 64;
