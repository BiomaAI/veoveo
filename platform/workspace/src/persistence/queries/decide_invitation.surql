LET $current_chat = SELECT * FROM ONLY $command.chat;
LET $current_member = SELECT * FROM ONLY $command.member;
LET $invitation = SELECT * FROM ONLY $command.invitation;
IF $invitation = NONE OR $invitation.chat != $command.chat
    OR $current_chat.tenant != $authority.tenant
    OR $current_chat.work_context != $authority.work_context { THROW 'workspace_not_found'; };
IF $command.state = 'revoked' {
    IF !fn::workspace_member($authority, $command.chat)
        OR $current_chat.owner != $authority.principal { THROW 'workspace_forbidden'; };
} ELSE IF $invitation.invitee != $authority.principal { THROW 'workspace_not_found'; };
IF $invitation.state = $command.state {
    IF $command.state = 'accepted' AND !fn::workspace_member($authority, $command.chat) {
        THROW 'workspace_not_found';
    };
} ELSE {
    IF $invitation.state != 'pending' OR $invitation.expires_at <= time::now()
        OR $current_chat.archived { THROW 'workspace_conflict'; };
    IF $command.state = 'accepted' {
        -- A removed inviter cannot leave reusable access invitations behind.
        IF count(SELECT id FROM workspace_member WHERE chat = $command.chat
            AND principal = $invitation.inviter AND active = true LIMIT 1) != 1 {
            THROW 'workspace_forbidden';
        };
        IF $invitation.inviter != $current_chat.owner AND !$current_chat.members_can_invite {
            THROW 'workspace_forbidden';
        };
        IF count(SELECT id FROM workspace_member WHERE chat = $command.chat LIMIT 256) >= 256
            AND $current_member.chat != $command.chat { THROW 'workspace_conflict'; };
        UPSERT ONLY $command.member SET chat = $command.chat, principal = $authority.principal,
            active = true, joined_at = time::now();
        LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    LET $sequence = $head.sequence;
    CREATE ONLY type::record('workspace_event', [$command.chat, $sequence]) SET
        chat = $command.chat, sequence = $sequence, kind = 'membership', target = $command.member;
    };
    UPDATE ONLY $command.invitation SET state = $command.state;
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    LET $sequence = $head.sequence;
    CREATE ONLY type::record('workspace_event', [$command.chat, $sequence]) SET
        chat = $command.chat, sequence = $sequence, kind = 'invitation', target = $command.invitation;
};
RETURN SELECT * FROM ONLY $command.invitation;
