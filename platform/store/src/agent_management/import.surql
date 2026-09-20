IF !$authority.manage_context { THROW 'agent_forbidden'; };
IF array::len(SELECT id FROM workspace_run WHERE chat.tenant = $authority.tenant
    AND chat.work_context = $authority.work_context AND state IN ['queued', 'running'] LIMIT 1) > 0 {
    THROW 'agent_conflict';
};
FOR $entry IN $command.entries {
    LET $before = $entry.before;
    LET $current = SELECT * FROM ONLY $before.id;
    LET $chat = SELECT * FROM ONLY $before.chat;
    IF $chat.tenant != $authority.tenant OR $chat.work_context != $authority.work_context { THROW 'agent_forbidden'; };
    LET $converted = object::extend($before, { definition_digest: $entry.target_digest });
    LET $expected = IF $command.direction = 'apply' { $before } ELSE { $converted };
    LET $target = IF $command.direction = 'apply' { $converted } ELSE { $before };
    IF $current != $expected AND $current != $target { THROW 'agent_conflict'; };
    IF $command.direction = 'apply' AND !fn::agent_chat_revision($authority.tenant, $authority.work_context,
        $before.definition, $entry.target_digest, false) { THROW 'agent_not_found'; };
    IF $current != $target {
        UPDATE ONLY $before.id SET definition_digest = $target.definition_digest;
        LET $head = UPDATE ONLY $before.chat SET sequence += 1, updated_at = time::now();
        CREATE ONLY type::record('workspace_event', [$before.chat, $head.sequence]) SET
            chat = $before.chat, sequence = $head.sequence, kind = 'agent', target = $before.id;
    };
};
CREATE outbox_event SET tenant = $authority.tenant, aggregate_type = 'agent_catalog_import',
    aggregate_id = <string> $authority.work_context, event_type = string::concat('agent_catalog_import.', $command.direction),
    schema_version = 1, payload = { work_context: $authority.work_context, actor: $authority.principal, bindings: array::len($command.entries) };
RETURN array::len($command.entries);
