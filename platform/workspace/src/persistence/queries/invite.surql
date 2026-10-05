LET $current_chat = SELECT * FROM ONLY $command.chat;
LET $current_invitee = SELECT * FROM ONLY $command.invitee;
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
IF $current_chat.owner != $authority.principal AND !$current_chat.members_can_invite {
    THROW 'workspace_forbidden';
};
IF $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
IF $current_invitee.tenant != $authority.tenant OR $current_invitee.enabled != true
    OR $current_invitee.kind != 'user' { THROW 'workspace_not_found'; };
LET $existing = SELECT * FROM ONLY $command.invitation;
IF $existing != NONE {
    IF $existing.chat != $command.chat { THROW 'workspace_not_found'; };
    IF $existing.inviter != $authority.principal
        OR $existing.invitee != $command.invitee { THROW 'workspace_conflict'; };
} ELSE {
    IF $current_chat.archived { THROW 'workspace_conflict'; };
    IF count(SELECT id FROM workspace_member WHERE chat = $command.chat
        AND principal = $command.invitee AND active = true LIMIT 1) > 0 {
        THROW 'workspace_conflict';
    };
    CREATE ONLY $command.invitation SET chat = $command.chat,
        inviter = $authority.principal, invitee = $command.invitee, expires_at = $command.expires_at;
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    LET $sequence = $head.sequence;
    CREATE ONLY type::record('workspace_event', [$command.chat, $sequence]) SET
        chat = $command.chat, sequence = $sequence, kind = 'invitation', target = $command.invitation;
};
RETURN SELECT * FROM ONLY $command.invitation;
