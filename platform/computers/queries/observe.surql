LET $current = SELECT * FROM ONLY $operation;
LET $computer_state = SELECT * FROM ONLY $computer;
IF $current = NONE OR $computer_state = NONE
  OR $current.operation_id != $operation_id
  OR $computer_state.active_operation != $operation_id
  OR $current.provider_instance_id != $provider
  OR $computer_state.provider_instance_id != $provider
  OR $current.template_fingerprint != $computer_state.template_fingerprint
  OR $current.previous_resource_id != $computer_state.provider_resource_id
  OR $current.previous_process_id != $computer_state.process_id
  OR $current.dispatch_id = NONE {
  THROW "computer_state_conflict";
};
IF $current.stage = "dispatched" {
  IF $current.observation_deadline = NONE OR $current.observation_deadline <= time::now()
    OR $current.observation_reads >= 8 {
    LET $changed = (UPDATE ONLY $operation SET stage = "recovery_required", updated_at = time::now()
      WHERE stage = "dispatched" AND updated_at = $current.updated_at RETURN AFTER);
    IF $changed = NONE { THROW "computer_state_conflict"; };
    LET $marked = (UPDATE ONLY $computer SET phase = "recovery_required", updated_at = time::now()
      WHERE active_operation = $operation_id AND updated_at = $computer_state.updated_at RETURN AFTER);
    IF $marked = NONE { THROW "computer_state_conflict"; };
    CREATE outbox_event CONTENT $event;
  } ELSE IF $current.next_observation_at = NONE OR $current.next_observation_at <= time::now() {
    LET $charged = (UPDATE ONLY $operation SET observation_reads += 1, last_observation_id = $observation_id,
      next_observation_at = time::now() + $backoff, updated_at = time::now()
      WHERE stage = "dispatched" AND updated_at = $current.updated_at AND observation_reads < 8
        AND observation_deadline > time::now()
        AND (next_observation_at = NONE OR next_observation_at <= time::now()) RETURN AFTER);
    IF $charged = NONE { THROW "computer_state_conflict"; };
  };
} ELSE IF $current.stage != "recovery_required" { THROW "computer_state_conflict"; };
