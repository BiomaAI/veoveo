SELECT * FROM task WHERE id IN $tasks
    AND server = $server AND tenant = $tenant
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

    AND task_type IN $task_types AND work_context = $work_context
    AND (IF type::is_object(authority) THEN
        IF type::is_object(request) THEN
            IF type::is_object(owner_context) THEN
                IF type::is_object(owner_context.authority) THEN
                    authority.context_key = $work_context_key
                    AND owner_context.authority.work_context = $work_context_key
                    AND owner_context.authority.tenant = $authority_tenant
                ELSE false END
            ELSE false END
        ELSE false END
    ELSE false END)
;
