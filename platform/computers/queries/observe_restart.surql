BEGIN TRANSACTION;
LET $now = time::now();
LET $current = SELECT * FROM ONLY $computer;
IF $expires_at <= $now OR $current = NONE OR $current.updated_at != $updated_at
  OR $current.owner_key != $owner_key OR $current.owner_context != $owner_context
  OR $current.provider_instance_id != $provider OR $current.replacement_instance_id != $instance
  OR $current.template_fingerprint != $template OR $current.provider_resource_id != $resource
  OR $current.process_id != $previous_process OR $current.phase != "ready" {
  THROW "computer_state_conflict";
};
IF $current.active_operation != NONE OR (SELECT * FROM ONLY $slot) != NONE {
  THROW "computer_operation_busy";
};
UPDATE $computer SET process_id = $process, updated_at = $now;
UPDATE computer_session_grant SET revoked_at = $now
  WHERE computer_id = $computer_id AND process_id = $previous_process AND revoked_at = NONE;
CREATE outbox_event CONTENT $event;
COMMIT TRANSACTION;
