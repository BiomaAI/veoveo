LET $current = SELECT * FROM ONLY $execution;
LET $slot_state = SELECT * FROM ONLY $slot;
IF $current = NONE OR $current.execution_id != $execution_id OR $current.provider_instance_id != $provider
  OR $current.task != $_provider_task OR $slot_state.execution != $execution
  OR $current.containment_id = NONE OR $current.stage NOT IN ["containing", "recovery_required"]
  { THROW "computer_state_conflict"; };
IF $current.stage = "containing" {
  IF $current.containment_deadline <= time::now() OR $current.containment_reads >= 8 {
    LET $changed = (UPDATE ONLY $execution SET stage = "recovery_required", updated_at = time::now()
      WHERE stage = "containing" AND updated_at = $current.updated_at RETURN AFTER);
    IF $changed = NONE { THROW "computer_state_conflict"; };
    -- Preserve an independently owned lifecycle fence and its phase.
    UPDATE $computer SET phase = "recovery_required", updated_at = time::now()
      WHERE active_operation = NONE AND provider_instance_id = $provider
        AND template_fingerprint = $current.binding.template_fingerprint
        AND provider_resource_id = $current.binding.resource_id AND process_id = $current.binding.process_id
        AND phase != "stopped";
    CREATE outbox_event CONTENT $event;
  } ELSE IF $current.next_containment_read = NONE OR $current.next_containment_read <= time::now() {
    LET $charged = (UPDATE ONLY $execution SET containment_reads += 1, last_containment_read_id = $read_id,
      next_containment_read = time::now() + $backoff, updated_at = time::now()
      WHERE stage = "containing" AND updated_at = $current.updated_at AND containment_reads < 8
        AND containment_deadline > time::now()
        AND (next_containment_read = NONE OR next_containment_read <= time::now()) RETURN AFTER);
    IF $charged = NONE { THROW "computer_state_conflict"; };
  };
};
