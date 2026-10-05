SELECT * FROM domain_usage WHERE server = $server AND tenant = $tenant
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
 AND task.work_context = $work_context
    AND (IF type::is_object(task.authority) THEN
        IF type::is_object(task.request) THEN
            IF type::is_object(task.request.owner) THEN
                IF type::is_object(task.request.owner.authority) THEN
                    task.authority.context_key = $work_context_key
                    AND task.request.owner.authority.work_context = $work_context_key
                    AND task.request.owner.authority.tenant = $authority_tenant
                ELSE false END
            ELSE false END
        ELSE false END
    ELSE false END)
 AND task = $task ORDER BY recorded_at ASC, id ASC;
