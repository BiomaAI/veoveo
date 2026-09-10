LET $current = SELECT * FROM ONLY $execution;
LET $computer_state = SELECT * FROM ONLY $computer;
LET $slot_state = SELECT * FROM ONLY $slot;
IF $current = NONE OR $computer_state = NONE OR $slot_state.execution != $execution
  OR $current.execution_id != $execution_id OR $current.task != $_provider_task
  OR $current.provider_instance_id != $provider OR $computer_state.provider_instance_id != $provider
  OR $current.computer_id != $computer_state.computer_id
  OR $computer_state.owner_key != $current.binding.owner_key
  OR $computer_state.template_fingerprint != $current.binding.template_fingerprint
  OR $computer_state.provider_resource_id != $current.binding.resource_id
  OR $computer_state.process_id != $current.binding.process_id
  OR $current.stage != "containing" OR $current.containment_id = NONE
  { THROW "computer_state_conflict"; };
IF $current.containment_dispatch_id = NONE AND $current.containment_deadline > time::now()
  AND $computer_state.active_operation = NONE AND $computer_state.phase = "ready" {
  LET $changed = (UPDATE ONLY $execution SET containment_dispatch_id = $stop_id, updated_at = time::now()
    WHERE stage = "containing" AND containment_dispatch_id = NONE AND containment_deadline > time::now() RETURN AFTER);
  IF $changed = NONE { THROW "computer_state_conflict"; };
  LET $fenced = (UPDATE ONLY $computer SET phase = "stopping", updated_at = time::now()
    WHERE phase = "ready" AND active_operation = NONE AND updated_at = $computer_state.updated_at RETURN AFTER);
  IF $fenced = NONE { THROW "computer_state_conflict"; };
  CREATE outbox_event CONTENT $event;
};
