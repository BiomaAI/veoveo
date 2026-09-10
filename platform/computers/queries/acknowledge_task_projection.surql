LET $current = SELECT * FROM ONLY $operation;
LET $task_state = SELECT * FROM ONLY $task;
IF $current = NONE OR $current.provider_instance_id != $provider
  OR $current.stage != $status OR $current.task != $task
  OR $task_state = NONE OR $task_state.status != $status {
  THROW 'computer_state_conflict';
};
UPDATE ONLY $operation SET task_projected_at = task_projected_at ?? time::now() WHERE stage = $status AND provider_instance_id = $provider;
