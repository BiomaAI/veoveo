IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
LET $chat = SELECT * FROM ONLY $command.chat;
RETURN {
    chat: $chat,
    members: (SELECT * FROM workspace_member WHERE chat = $command.chat LIMIT 256),
    messages: (SELECT * FROM workspace_message WHERE chat = $command.chat
        AND sequence < $command.after AND sequence <= $chat.sequence
        ORDER BY sequence DESC LIMIT $command.limit),
};
