SELECT VALUE task FROM domain_usage WHERE server = $server AND tenant = $tenant
    AND task.server = $server AND task.tenant = $tenant
    AND task.owner = $owner AND task.profile = $profile
    AND (IF type::is_object(task.request) THEN
        IF type::is_object(task.owner_context) THEN
            IF type::is_array(task.owner_context.data_labels) THEN
                task.owner_context.principal_key = $principal_key
                AND task.owner_context.profile = $profile_key
                AND (task.owner_context.tenant_key ?? NONE) = $tenant_key
                AND task.owner_context.data_labels ALLINSIDE $labels
            ELSE false END
        ELSE false END
    ELSE false END)
   GROUP BY task ORDER BY task ASC LIMIT $limit;
