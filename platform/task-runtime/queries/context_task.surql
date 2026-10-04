AND work_context = $work_context
    AND (IF type::is_object(authority) THEN
        IF type::is_object(request) THEN
            IF type::is_object(request.owner) THEN
                IF type::is_object(request.owner.authority) THEN
                    authority.context_key = $work_context_key
                    AND request.owner.authority.work_context = $work_context_key
                    AND request.owner.authority.tenant = $authority_tenant
                ELSE false END
            ELSE false END
        ELSE false END
    ELSE false END)
