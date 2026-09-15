LET $current_chat = SELECT * FROM ONLY $command.chat;
LET $current_member = SELECT * FROM ONLY $command.member;
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
IF $current_member.chat != $command.chat { THROW 'workspace_not_found'; };
IF $current_chat.owner != $authority.principal AND $current_member.principal != $authority.principal {
    THROW 'workspace_forbidden';
};
IF $current_member.principal = $current_chat.owner { THROW 'workspace_conflict'; };
IF $current_member.active {
    UPDATE ONLY $command.member SET active = false;
    UPDATE workspace_invitation SET state = 'revoked' WHERE chat = $command.chat
        AND state = 'pending'
        AND (invitee = $current_member.principal OR inviter = $current_member.principal);
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    LET $sequence = $head.sequence;
    CREATE ONLY type::record('workspace_event', [$command.chat, $sequence]) SET
        chat = $command.chat, sequence = $sequence, kind = 'membership', target = $command.member;
};
RETURN SELECT * FROM ONLY $command.member;
