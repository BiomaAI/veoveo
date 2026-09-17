LET $old = SELECT * FROM ONLY $command.operation;
IF $old = NONE OR $old.phase != 'dispatching' OR $old.fence != $command.fence
    OR $old.dispatch_until <= time::now() { THROW 'workspace_conflict'; };
IF $old.progress = NONE OR $command.progress.completed > $old.progress.completed {
    UPDATE ONLY $command.operation SET progress = $command.progress, revision += 1, updated_at = time::now();
};
RETURN true;
