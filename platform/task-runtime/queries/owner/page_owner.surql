SELECT * FROM task WHERE server = $server AND tenant = $tenant
    AND owner = $owner AND profile = $profile
    AND (IF type::is_object(request) THEN
        IF type::is_object(owner_context) THEN
            IF type::is_array(owner_context.data_labels) THEN
                owner_context.principal_key = $principal_key
                AND owner_context.profile = $profile_key
                AND (owner_context.tenant_key ?? NONE) = $tenant_key
                AND owner_context.data_labels ALLINSIDE $labels
            ELSE false END
        ELSE false END
    ELSE false END)
    ORDER BY created_at ASC, id ASC LIMIT $limit;
