BEGIN TRANSACTION;
LET $updated = (
    UPDATE ONLY $task SET
        status = $status, request = $request, error = $error,
        lease_owner = NONE, lease_expires_at = NONE,
        completed_at = $completed_at, updated_at = $now
    WHERE status = $expected AND updated_at = $expected_updated_at AND request = $expected_request AND owner_context = $expected_owner_context
        AND (lease_expires_at = NONE OR lease_expires_at <= $now)
    RETURN AFTER
);
IF $updated != NONE AND $status = 'failed' {
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
