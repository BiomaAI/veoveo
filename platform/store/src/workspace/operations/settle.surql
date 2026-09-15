LET $old = SELECT * FROM ONLY $command.operation;
IF $old = NONE { THROW 'workspace_not_found'; };
IF $old.phase NOT IN ['dispatching', 'unconfirmed'] OR $old.fence != $command.fence {
    THROW 'workspace_conflict';
};
UPDATE ONLY $command.operation SET phase = $command.phase, task_id = $command.task_id,
    response = $command.response, revision += 1, updated_at = time::now();
RETURN SELECT * FROM ONLY $command.operation;
