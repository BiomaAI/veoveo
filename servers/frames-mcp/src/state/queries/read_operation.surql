SELECT * FROM coordinate_operation WHERE id = $operation AND operation_key = $operation_key
    AND tenant = $tenant AND owner = $owner
    AND authority.profile = $profile AND (authority.tenant_key ?? NONE) = $tenant_key
    AND labels ALLINSIDE $clearance
    AND IF task = NONE THEN true ELSE
        fn::kernel::tasks::selection_v1(
            task, mcp_server:frames, $tenant, $owner, $profile,
            $principal_key, $profile_key, $tenant_key, $clearance,
            NONE, NONE, NONE, NONE) != NONE
    END LIMIT 1;
