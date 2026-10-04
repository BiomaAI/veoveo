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
