LET $chat = SELECT * FROM ONLY $command.chat;
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
IF $chat.owner != $authority.principal OR $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
IF $chat.archived { THROW 'workspace_conflict'; };
LET $existing = SELECT * FROM ONLY $command.agent;
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
RETURN SELECT * FROM ONLY $command.agent;
