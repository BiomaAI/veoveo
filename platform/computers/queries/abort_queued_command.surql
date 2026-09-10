LET $current = SELECT * FROM ONLY $execution;
LET $slot_state = SELECT * FROM ONLY $slot;
IF $current = NONE OR $current.execution_id != $execution_id OR $current.provider_instance_id != $provider
  OR $current.task != $_provider_task OR $slot_state.execution != $execution
  OR $current.stage != "queued" OR $current.dispatch_id != NONE
  OR ($refusal = "cancelled_before_dispatch" AND $_provider_guard.status != "cancel_requested")
  { THROW "computer_state_conflict"; };
LET $now = time::now();
LET $stage = IF $refusal = "cancelled_before_dispatch" { "cancelled" } ELSE { "failed" };
LET $aborted = (UPDATE ONLY $execution SET stage = $stage, refusal = $refusal, settled_at = $now, updated_at = $now
  WHERE stage = "queued" AND dispatch_id = NONE AND updated_at = $current.updated_at RETURN AFTER);
IF $aborted = NONE { THROW "computer_state_conflict"; };
DELETE $slot WHERE execution = $execution;
CREATE outbox_event CONTENT $event;
