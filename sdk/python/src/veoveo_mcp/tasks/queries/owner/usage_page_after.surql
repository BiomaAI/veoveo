SELECT VALUE task FROM domain_usage WHERE server = $server AND tenant = $tenant AND task.server = $server AND task.tenant = $tenant AND task.owner = $owner
    AND task.profile = $profile AND task.request.owner.principal_key = $principal_key
    AND task.request.owner.profile = $profile_key
    AND (task.request.owner.tenant_key ?? NONE) = $tenant_key
    AND task.request.owner.data_labels ALLINSIDE $labels AND task > $after AND task != $after GROUP BY task ORDER BY task ASC LIMIT $limit;