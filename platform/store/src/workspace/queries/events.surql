LET $current_chat = SELECT * FROM ONLY $command.chat;
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
LET $head = $current_chat.sequence;
IF $command.after > $head { THROW 'workspace_conflict'; };
RETURN {
    through_sequence: $head,
    events: (SELECT chat, sequence, kind, target, created_at FROM workspace_event
        WHERE chat = $command.chat AND sequence > $command.after AND sequence <= $head
        ORDER BY sequence ASC LIMIT $command.limit),
};
