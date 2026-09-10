BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $execution;
LET $task_state = SELECT * FROM ONLY $task;
IF $current = NONE OR $current.provider_instance_id != $provider
  OR $current.stage != $stage OR $current.task != $task OR $current.settled_at = NONE
  OR $task_state = NONE OR $task_state.status != $status { THROW "computer_state_conflict"; };
IF $stage = "completed" AND ($task_state.result.structuredContent != $current.result
  OR ($task_state.result.isError ?? false) != ($current.result.exitCode != 0)) { THROW "computer_state_conflict"; };
UPDATE ONLY $execution SET task_projected_at = task_projected_at ?? time::now()
  WHERE stage = $stage AND provider_instance_id = $provider;
COMMIT TRANSACTION;
