RETURN {
    catalog: (SELECT key, revision FROM agent_definition
        WHERE tenant = $authority.tenant AND status = 'enabled' AND disabled = false
          AND $authority.work_context IN audience AND published != NONE ORDER BY key),
    definitions: (SELECT key, revision FROM agent_definition
        WHERE tenant = $authority.tenant AND work_context = $authority.work_context
          AND (owner = $authority.principal OR $authority.manage_context) ORDER BY key),
    instances: (SELECT key, generation, dispatch_epoch, updated_at FROM managed_agent
        WHERE tenant = $authority.tenant AND work_context = $authority.work_context
          AND (owner = $authority.principal OR $authority.manage_context) ORDER BY key)
};
