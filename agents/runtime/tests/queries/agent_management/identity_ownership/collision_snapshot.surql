RETURN [
    array::len(SELECT id FROM principal WHERE tenant = $tenant),
    array::len(SELECT id FROM managed_agent WHERE tenant = $tenant),
    array::len(SELECT id FROM managed_agent_operation WHERE tenant = $tenant),
    array::first(SELECT VALUE instances FROM managed_agent_capacity WHERE work_context = $context) ?? 0,
    array::first(SELECT VALUE storage_gib FROM managed_agent_capacity WHERE work_context = $context) ?? 0
];
