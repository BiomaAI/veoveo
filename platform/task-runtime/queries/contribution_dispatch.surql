LET $_dispatch_row = SELECT * FROM ONLY $_contribution_row;
IF $_dispatch_row = NONE OR $_dispatch_row.task != $_contribution_task
    OR $_dispatch_row.task_type != $_contribution_type
    OR $_dispatch_row.created_at != $_contribution_created
    OR $_dispatch_row.identity != $_contribution_identity {
    THROW 'task_dispatch_receipt_conflict';
};
IF $_dispatch_row.dispatch != NONE AND $_dispatch_row.dispatch != $_contribution_dispatch {
    THROW 'task_dispatch_identity_conflict';
};
LET $_dispatch_new = $_dispatch_row.dispatch = NONE;
IF $_dispatch_new {
    UPDATE ONLY $_contribution_row SET dispatch = $_contribution_dispatch;
};
