LET $chat = SELECT * FROM ONLY $command.chat;
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
IF $chat.owner != $authority.principal OR $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
IF $chat.archived { THROW 'workspace_conflict'; };
LET $receipt = SELECT * FROM ONLY $command.receipt;
IF $receipt != NONE {
    IF $receipt.chat != $command.chat OR $receipt.actor != $authority.principal OR $receipt.fingerprint != $command.fingerprint { THROW 'workspace_conflict'; };
    RETURN $receipt.result;
};
LET $agent = SELECT * FROM ONLY $command.agent;
IF $agent.chat != $command.chat OR !$agent.active { THROW 'workspace_not_found'; };
IF $agent.definition_digest != $command.expected { THROW 'workspace_conflict'; };
IF !fn::agent_chat_revision($authority.tenant, $authority.work_context, $agent.definition, $command.admission.definition_digest, false) { THROW 'workspace_not_found'; };
UPDATE ONLY $command.agent SET definition_digest = $command.admission.definition_digest,
    display_name = $command.admission.display_name, provider = $command.admission.provider, model = $command.admission.model;
LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
CREATE ONLY type::record('workspace_event', [$command.chat, $head.sequence]) SET
    chat = $command.chat, sequence = $head.sequence, kind = 'agent', target = $command.agent;
LET $result = SELECT * FROM ONLY $command.agent;
CREATE ONLY $command.receipt SET chat = $command.chat, actor = $authority.principal, fingerprint = $command.fingerprint, result = $result;
RETURN $result;
