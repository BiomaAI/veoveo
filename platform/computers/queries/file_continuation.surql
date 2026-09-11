LET $current = SELECT provider_instance_id, task, stage, dispatch_id, binding, authority,
  effective_limits, created_at, dispatched_at, execution_deadline FROM ONLY $execution;
RETURN $current != NONE AND $current.provider_instance_id = $provider AND $current.task = $task
  AND $current.stage = $stage AND $current.dispatch_id = $dispatch_id
  AND $current.binding = $binding AND $current.authority = $authority
  AND $current.effective_limits = $limits AND $current.dispatched_at = $dispatched_at
  AND $current.created_at = $created_at
  AND $current.execution_deadline = $execution_deadline;
RETURN time::now();
