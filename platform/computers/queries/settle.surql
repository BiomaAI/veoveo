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
  OR $current.stage NOT IN ["dispatched", "recovery_required"]
  OR ($evidence_kind = "dispatch" AND $current.dispatch_id != $evidence_id)
  OR ($evidence_kind = "observation" AND $current.last_observation_id != $evidence_id) {
  THROW "computer_state_conflict";
};
IF ($current.action = "start" AND ($resource != $current.previous_resource_id OR $process = $current.previous_process_id))
  OR ($current.action = "stop" AND ($resource != $current.previous_resource_id OR $process != $current.previous_process_id)) {
  THROW "computer_state_conflict";
};
LET $settled = (UPDATE ONLY $operation SET stage = "succeeded", result_resource_id = $resource,
  result_process_id = $process, settled_at = time::now(), updated_at = time::now()
  WHERE stage IN ["dispatched", "recovery_required"] AND updated_at = $current.updated_at RETURN AFTER);
IF $settled = NONE { THROW "computer_state_conflict"; };
LET $released = (UPDATE ONLY $computer SET phase = $phase, provider_resource_id = $resource, process_id = $process,
  active_operation = NONE, updated_at = time::now()
  WHERE active_operation = $operation_id AND updated_at = $computer_state.updated_at RETURN AFTER);
IF $released = NONE { THROW "computer_state_conflict"; };
CREATE outbox_event CONTENT $event;
