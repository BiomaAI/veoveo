LET $_association_updated = (
    UPDATE ONLY $_contribution_row SET association = $_contribution_association
    WHERE task = $_contribution_task
        AND task_type = $_contribution_type
        AND created_at = $_contribution_created
        AND identity = $_contribution_identity
        AND dispatch != NONE
        AND (association = NONE OR association = $_contribution_association)
    RETURN AFTER
);
IF $_association_updated = NONE { THROW 'task_provider_association_conflict'; };
