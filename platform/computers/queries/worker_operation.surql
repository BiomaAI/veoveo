RETURN {
  LET $exists = SELECT VALUE id FROM ONLY $journal;
  LET $admitted = SELECT * FROM ONLY $journal
  WHERE id != NONE AND provider_instance_id = $provider
    AND computer_id = $computer_id AND task = $task
    AND operation_id = $task_id AND actor_context = $actor;
  IF $exists != NONE AND $admitted = NONE { THROW "computer_state_conflict"; };
  $admitted;
};
RETURN time::now();
