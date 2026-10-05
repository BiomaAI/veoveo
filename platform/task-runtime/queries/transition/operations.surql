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
 AND task_type IN $task_types
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
