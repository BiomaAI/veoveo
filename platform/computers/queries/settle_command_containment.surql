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
  OR $resource != $current.binding.resource_id OR $process != $current.binding.process_id
  OR $current.stage NOT IN ["containing", "recovery_required"] OR $current.containment_id != $containment_id
  OR ($evidence_kind = "stop" AND $current.containment_dispatch_id != $evidence_id)
  OR ($evidence_kind = "read" AND $current.last_containment_read_id != $evidence_id)
  { THROW "computer_state_conflict"; };
LET $now = time::now();
LET $stage = IF $current.interruption = "cancelled" { "cancelled" } ELSE { "failed" };
LET $settled = (UPDATE ONLY $execution SET stage = $stage, terminated_at = $now, settled_at = $now, updated_at = $now,
  termination_evidence = {kind:$evidence_kind, id:<string>$evidence_id}
  WHERE stage IN ["containing", "recovery_required"] AND updated_at = $current.updated_at RETURN AFTER);
IF $settled = NONE { THROW "computer_state_conflict"; };
IF $computer_state.active_operation = NONE {
  LET $stopped = (UPDATE ONLY $computer SET phase = "stopped", updated_at = $now
    WHERE active_operation = NONE AND updated_at = $computer_state.updated_at RETURN AFTER);
  IF $stopped = NONE { THROW "computer_state_conflict"; };
};
DELETE $slot WHERE execution = $execution;
CREATE outbox_event CONTENT $event;
