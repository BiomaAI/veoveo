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
