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
    WHERE status = $expected AND updated_at = $expected_updated_at
        AND server = $server AND tenant = $tenant AND owner = $owner
        AND server = $server AND tenant = $tenant
    AND owner = $owner AND profile = $profile
    AND (IF type::is_object(request) THEN
        IF type::is_object(request.owner) THEN
            IF type::is_array(request.owner.data_labels) THEN
                request.owner.principal_key = $principal_key
                AND request.owner.profile = $profile_key
                AND (request.owner.tenant_key ?? NONE) = $tenant_key
                AND request.owner.data_labels ALLINSIDE $labels
            ELSE false END
        ELSE false END
    ELSE false END)
 AND task_type IN $task_types AND work_context = $work_context
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

        AND (
            $control_transition
            OR (lease_owner = $worker AND lease_expires_at > $now)
            OR ($expired_cancellation AND (lease_expires_at = NONE OR lease_expires_at <= $now))
        )
    RETURN AFTER
);
IF $updated != NONE AND $terminal {
    LET $_contribution_updated = (
    UPDATE ONLY $_contribution_row SET settlement = $_contribution_settlement
    WHERE task = $_contribution_task
        AND task_type = $_contribution_type
        AND created_at = $_contribution_created
        AND identity = $_contribution_identity
        AND settlement = NONE
    RETURN AFTER
);
IF $_contribution_updated = NONE { THROW 'task_contribution_conflict'; };

};
RETURN $updated;
COMMIT TRANSACTION;
