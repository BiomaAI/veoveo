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
