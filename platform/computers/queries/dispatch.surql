LET $current = SELECT * FROM ONLY $operation;
LET $computer_state = SELECT * FROM ONLY $computer;
IF $current = NONE OR $computer_state = NONE
  OR $current.stage != "queued" OR $current.dispatch_id != NONE
  OR $current.operation_id != $operation_id
  OR $current.computer_id != $computer_state.computer_id
  OR $computer_state.active_operation != $operation_id
  OR $current.provider_instance_id != $provider
  OR $computer_state.provider_instance_id != $provider
  OR $current.template_fingerprint != $computer_state.template_fingerprint
  OR $current.previous_resource_id != $computer_state.provider_resource_id
  OR $current.previous_process_id != $computer_state.process_id {
  THROW "computer_state_conflict";
};
LET $dispatched = (UPDATE ONLY $operation SET stage = "dispatched", dispatch_id = $dispatch_id,
  dispatched_at = time::now(), observation_deadline = time::now() + 180s,
  updated_at = time::now() WHERE stage = "queued" AND dispatch_id = NONE
    AND updated_at = $current.updated_at RETURN AFTER);
IF $dispatched = NONE { THROW "computer_state_conflict"; };
CREATE outbox_event CONTENT $event;
