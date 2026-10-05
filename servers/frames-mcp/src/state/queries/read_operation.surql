SELECT * FROM coordinate_operation WHERE id = $operation AND operation_key = $operation_key
    AND tenant = $tenant AND owner = $owner
    AND authority.profile = $profile AND (authority.tenant_key ?? NONE) = $tenant_key
    AND labels ALLINSIDE $clearance
    AND (task = NONE OR (task.server = mcp_server:frames
        AND task.tenant = $tenant AND task.owner = $owner AND task.profile = $profile
        AND task.request.owner.principal_key = $principal_key
        AND task.request.owner.profile = $profile_key
        AND (task.request.owner.tenant_key ?? NONE) = $tenant_key
        AND task.request.owner.data_labels ALLINSIDE $clearance)) LIMIT 1;
