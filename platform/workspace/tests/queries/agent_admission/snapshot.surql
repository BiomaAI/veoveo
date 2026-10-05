RETURN {
    agents: (SELECT * FROM workspace_agent WHERE chat.tenant = $tenant ORDER BY id),
    chats: (SELECT * FROM workspace_chat WHERE tenant = $tenant ORDER BY id),
    events: (SELECT * FROM workspace_event WHERE chat.tenant = $tenant ORDER BY id),
    receipts: (SELECT * FROM workspace_agent_revision_receipt WHERE chat.tenant = $tenant ORDER BY id)
};
