LET $current_chat = SELECT * FROM ONLY $command.chat;
LET $current_owner = SELECT * FROM ONLY $command.owner;
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
IF $current_chat.owner != $authority.principal OR $authority.membership = 'viewer' {
    THROW 'workspace_forbidden';
};
IF $current_chat.revision != $command.expected_revision { THROW 'workspace_conflict'; };
IF $current_owner.enabled != true OR $current_owner.kind != 'user'
    OR count(SELECT id FROM workspace_member WHERE chat = $command.chat
        AND principal = $command.owner AND active = true LIMIT 1) != 1 { THROW 'workspace_not_found'; };
UPDATE ONLY $command.chat SET owner = $command.owner, title = $command.title,
    archived = $command.archived, members_can_invite = $command.members_can_invite, revision += 1;
LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    LET $sequence = $head.sequence;
    CREATE ONLY type::record('workspace_event', [$command.chat, $sequence]) SET
        chat = $command.chat, sequence = $sequence, kind = 'settings', target = $command.chat;
RETURN SELECT * FROM ONLY $command.chat;
