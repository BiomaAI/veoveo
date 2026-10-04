server = $server AND tenant = $tenant
    AND task.server = $server AND task.tenant = $tenant
    AND task.owner = $owner AND task.profile = $profile
    AND (IF type::is_object(task.request) THEN
        IF type::is_object(task.request.owner) THEN
            IF type::is_array(task.request.owner.data_labels) THEN
                task.request.owner.principal_key = $principal_key
                AND task.request.owner.profile = $profile_key
                AND (task.request.owner.tenant_key ?? NONE) = $tenant_key
                AND task.request.owner.data_labels ALLINSIDE $labels
            ELSE false END
        ELSE false END
    ELSE false END)
