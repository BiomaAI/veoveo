LET $chat = SELECT * FROM ONLY $command.chat;
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
IF $chat.owner != $authority.principal OR $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
IF $chat.archived { THROW 'workspace_conflict'; };
LET $receipt = SELECT * FROM ONLY $command.receipt;
IF $receipt != NONE {
    IF $receipt.chat != $command.chat OR $receipt.actor != $authority.principal OR $receipt.fingerprint != $command.fingerprint { THROW 'workspace_conflict'; };
    RETURN $receipt.result;
};
IF !fn::agent_chat_revision($authority.tenant, $authority.work_context, $command.admission.definition, $command.admission.definition_digest, true) { THROW 'workspace_not_found'; };
LET $existing = SELECT * FROM ONLY $command.agent;
IF $existing.active AND $existing.definition_digest != $command.admission.definition_digest { THROW 'workspace_conflict'; };
IF $existing = NONE AND count(SELECT id FROM workspace_agent WHERE chat = $command.chat) >= 64 {
    THROW 'workspace_conflict';
};
IF $existing = NONE OR !$existing.active OR $existing.definition_digest != $command.admission.definition_digest {
    UPSERT ONLY $command.agent SET chat = $command.chat,
        definition = $command.admission.definition, definition_digest = $command.admission.definition_digest,
        display_name = $command.admission.display_name, provider = $command.admission.provider,
        model = $command.admission.model, active = true;
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    CREATE ONLY type::record('workspace_event', [$command.chat, $head.sequence]) SET
        chat = $command.chat, sequence = $head.sequence, kind = 'agent', target = $command.agent;
};
LET $result = SELECT * FROM ONLY $command.agent;
CREATE ONLY $command.receipt SET chat = $command.chat, actor = $authority.principal, fingerprint = $command.fingerprint, result = $result;
RETURN $result;
