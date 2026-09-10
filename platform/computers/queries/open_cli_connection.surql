BEGIN TRANSACTION;
LET $now = time::now();
LET $current = (UPDATE ONLY $grant SET connections_revision += 1
  WHERE credential_hash = $credential_hash AND computer_id = $computer_id
  AND revoked_at = NONE AND expires_at > $now AND idle_expires_at > $now RETURN AFTER);
IF $current = NONE OR $authority_expires_at <= $now { THROW "computer_authority_expired"; };
LET $family = SELECT * FROM ONLY $current.family;
LET $policy_state = SELECT * FROM ONLY $policy;
IF $family = NONE OR $family.revoked_at != NONE OR $family.expires_at <= $now
  OR $policy_state = NONE OR $policy_state.max_grants = 0 { THROW "computer_authority_expired"; };
LET $state = SELECT * FROM ONLY $computer;
IF $state = NONE OR $state.owner_key != $current.owner_key OR $state.phase != "ready"
  OR $state.active_operation != NONE OR $state.provider_instance_id != $provider
  OR $current.provider_instance_id != $provider OR $state.provider_resource_id != $resource
  OR $state.process_id != $process { THROW "computer_invalid_state"; };
LET $active = SELECT VALUE connection_id FROM computer_cli_connection
  WHERE grant_id = $grant_id AND closed_at = NONE AND expires_at > $now LIMIT 16;
IF array::len($active) >= 16 { THROW "computer_access_limit"; };
CREATE $connection CONTENT {connection_id:$connection_id, grant_id:$grant_id,
  provider_resource_id:$resource, process_id:$process, issued_at:$now, expires_at:$authority_expires_at};
CREATE outbox_event CONTENT $event;
COMMIT TRANSACTION;
