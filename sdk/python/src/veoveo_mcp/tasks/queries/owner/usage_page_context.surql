SELECT VALUE task FROM domain_usage WHERE server = $server AND tenant = $tenant AND task.server = $server AND task.tenant = $tenant AND task.owner = $owner
    AND task.profile = $profile AND task.owner_context.principal_key = $principal_key
    AND task.owner_context.profile = $profile_key
    AND (task.owner_context.tenant_key ?? NONE) = $tenant_key
    AND task.owner_context.data_labels ALLINSIDE $labels AND task.work_context = $work_context
                AND task.authority.context_key = $work_context_key
                AND task.owner_context.authority.work_context = $work_context_key
                AND task.owner_context.authority.tenant = $authority_tenant  GROUP BY task ORDER BY task ASC LIMIT $limit;