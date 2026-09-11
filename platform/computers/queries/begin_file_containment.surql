LET $current = SELECT * FROM ONLY $execution;
LET $slot_state = SELECT * FROM ONLY $slot;
IF $current = NONE OR $current.transfer_id != $transfer_id OR $current.provider_instance_id != $provider
  OR $current.task != $_provider_task OR $slot_state.execution != $execution
  OR $current.dispatch_id = NONE OR $current.stage NOT IN ["dispatched", "containing", "recovery_required"]
  { THROW "computer_state_conflict"; };
IF $current.stage = "dispatched" {
  IF $interruption = "cancelled" AND $_provider_guard.status != "cancel_requested" { THROW "computer_state_conflict"; };
  IF $interruption = "deadline" AND $current.execution_deadline > time::now() { THROW "computer_state_conflict"; };
  LET $now = time::now();
  LET $changed = (UPDATE ONLY $execution SET stage = "containing", containment_id = $containment_id,
    interruption = $interruption, containment_started_at = $now, containment_deadline = $now + 180s,
    updated_at = $now WHERE stage = "dispatched" AND containment_id = NONE RETURN AFTER);
  IF $changed = NONE { THROW "computer_state_conflict"; };
  CREATE outbox_event CONTENT $event;
};
