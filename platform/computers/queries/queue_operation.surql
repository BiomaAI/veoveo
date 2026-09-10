BEGIN TRANSACTION;
LET $prior = SELECT * FROM ONLY $request;
IF $prior != NONE {
  IF $prior.fingerprint != $fingerprint { THROW "computer_request_conflict"; };
} ELSE {
  LET $current = SELECT * FROM ONLY $computer;
  IF $current = NONE OR $current.owner_key != $owner_key { THROW "computer_not_found"; };
  IF $current.active_operation != NONE { THROW "computer_operation_busy"; };
  IF $current.updated_at != $expected_updated_at
    OR $current.provider_instance_id != $content.provider_instance_id
    OR $current.template_fingerprint != $content.template_fingerprint {
    THROW "computer_state_conflict";
  };
  IF $current.phase != $previous_phase { THROW "computer_invalid_state"; };
  IF $previous_phase != "reserved" AND
    ($current.provider_resource_id = NONE OR $current.provider_resource_id = ""
     OR $current.process_id = NONE OR $current.process_id = "") {
    THROW "computer_invalid_state";
  };
  CREATE $operation CONTENT object::extend($content, {
    previous_phase: $current.phase,
    previous_resource_id: $current.provider_resource_id,
    previous_process_id: $current.process_id
  });
  CREATE $request CONTENT {operation_id: $content.operation_id, fingerprint: $fingerprint};
  UPDATE $computer SET active_operation = $content.operation_id, phase = $next_phase, updated_at = time::now();
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
