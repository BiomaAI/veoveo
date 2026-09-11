LET $now = time::now();
LET $current = SELECT * FROM ONLY $execution;
LET $computer_state = SELECT * FROM ONLY $computer;
LET $slot_state = SELECT * FROM ONLY $slot;
IF $current = NONE OR $computer_state = NONE OR $slot_state.execution != $execution
  OR $current.transfer_id != $transfer_id OR $current.task != $_provider_task
  OR $current.provider_instance_id != $provider OR $computer_state.provider_instance_id != $provider
  OR $current.computer_id != $computer_state.computer_id OR $current.binding != $binding
  OR $computer_state.owner_key != $current.binding.owner_key
  OR $computer_state.template_fingerprint != $current.binding.template_fingerprint
  OR <string>($computer_state.replacement_instance_id ?? $computer_state.computer_id) != $current.binding.instance_id
  OR $computer_state.provider_resource_id != $current.binding.resource_id
  OR $computer_state.process_id != $current.binding.process_id
  OR $current.stage != "dispatched" OR $current.dispatch_id != $dispatch_id
  OR $current.containment_id != NONE OR $current.artifact_access = NONE
  OR $now >= $current.execution_deadline + $publication_budget
  { THROW "computer_state_conflict"; };
IF ($result = NONE) = ($rejection = NONE) OR $rejection = "commit_unknown" { THROW "computer_state_conflict"; };
LET $stage = IF $result != NONE { "completed" } ELSE { "failed" };
LET $completed = (UPDATE ONLY $execution SET stage = $stage, result = $result, rejection = $rejection,
  settled_at = $now, updated_at = $now
  WHERE stage = "dispatched" AND dispatch_id = $dispatch_id AND updated_at = $current.updated_at RETURN AFTER);
IF $completed = NONE { THROW "computer_state_conflict"; };
-- Foreground completion does not change the Computer run or an owner's Stop.
DELETE $slot WHERE execution = $execution;
CREATE outbox_event CONTENT $event;
