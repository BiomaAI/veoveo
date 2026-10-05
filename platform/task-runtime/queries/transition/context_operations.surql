BEGIN TRANSACTION;
LET $updated = (
    UPDATE ONLY $task SET
        status = $next,
        request = $request,
        progress = $progress,
        result = $result,
        error = $error,
        cancel_requested_at = $cancel_requested_at,
        completed_at = $completed_at,
        lease_owner = IF $terminal { NONE } ELSE { lease_owner },
        lease_expires_at = IF $terminal { NONE } ELSE { lease_expires_at },
        updated_at = $now
    WHERE status = $expected AND updated_at = $expected_updated_at AND request = $expected_request AND owner_context = $expected_owner_context
        AND server = $server AND tenant = $tenant AND owner = $owner
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

        AND (
            $control_transition
            OR (lease_owner = $worker AND lease_expires_at > $now)
            OR ($expired_cancellation AND (lease_expires_at = NONE OR lease_expires_at <= $now))
        )
    RETURN AFTER
);
IF $updated != NONE AND $terminal {

};
RETURN $updated;
COMMIT TRANSACTION;
