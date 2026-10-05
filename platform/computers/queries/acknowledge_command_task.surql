BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $execution;
LET $task_state = fn::kernel::tasks::lifecycle_v1($task, $server, $task_tenant, $task_types, $expected_result);
IF $current = NONE OR $current.provider_instance_id != $provider
  OR $current.stage != $stage OR $current.task != $task OR $current.task_tenant != $task_tenant
  OR $current.settled_at = NONE OR $current.result != $expected_domain
  OR $task_state = NONE OR $task_state.status != $status { THROW "computer_state_conflict"; };
IF $stage = "completed" AND !$task_state.result_matches { THROW "computer_state_conflict"; };
UPDATE ONLY $execution SET task_projected_at = task_projected_at ?? time::now()
  WHERE stage = $stage AND provider_instance_id = $provider;
COMMIT TRANSACTION;
