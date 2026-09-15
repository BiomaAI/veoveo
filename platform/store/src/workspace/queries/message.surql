LET $current_chat = SELECT * FROM ONLY $command.chat;
LET $current_reply_to = IF $command.reply_to = NONE { NONE } ELSE {
    SELECT * FROM ONLY $command.reply_to
};
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
IF $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
LET $existing = SELECT * FROM ONLY $command.message;
IF $existing != NONE {
    IF $existing.chat != $command.chat { THROW 'workspace_not_found'; };
    IF $existing.author != $command.member
        OR $existing.text != $command.text OR $existing.reply_to != $command.reply_to {
        THROW 'workspace_conflict';
    };
} ELSE {
    IF $current_chat.archived { THROW 'workspace_conflict'; };
    IF $command.reply_to != NONE AND $current_reply_to.chat != $command.chat {
        THROW 'workspace_not_found';
    };
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    LET $sequence = $head.sequence;
    CREATE ONLY type::record('workspace_event', [$command.chat, $sequence]) SET
        chat = $command.chat, sequence = $sequence, kind = 'message', target = $command.message;
    CREATE ONLY $command.message SET chat = $command.chat, author = $command.member,
        text = $command.text, reply_to = $command.reply_to, sequence = $sequence;
};
RETURN SELECT * FROM ONLY $command.message;
