LET $current = SELECT * FROM ONLY $operation;
LET $computer_state = SELECT * FROM ONLY $computer;
IF $current = NONE OR $computer_state = NONE
  OR $current.operation_id != $operation_id OR $current.provider_instance_id != $provider
  OR $computer_state.active_operation != $operation_id
  OR $current.stage != "queued" OR $current.dispatch_id != NONE
  OR ($code = "cancelled_before_dispatch" AND $_provider_guard.status != "cancel_requested") {
  THROW "computer_state_conflict";
};
LET $aborted = (UPDATE ONLY $operation SET stage = $stage, completion_code = $code,
  settled_at = time::now(), updated_at = time::now()
  WHERE stage = "queued" AND dispatch_id = NONE AND updated_at = $current.updated_at RETURN AFTER);
IF $aborted = NONE { THROW "computer_state_conflict"; };
LET $released = (UPDATE ONLY $computer SET phase = $current.previous_phase,
  active_operation = NONE, updated_at = time::now()
  WHERE active_operation = $operation_id AND updated_at = $computer_state.updated_at RETURN AFTER);
IF $released = NONE { THROW "computer_state_conflict"; };
CREATE outbox_event CONTENT $event;
