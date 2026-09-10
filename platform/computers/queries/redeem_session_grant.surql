BEGIN TRANSACTION;
LET $now = time::now();
LET $current = SELECT * FROM ONLY $grant;
IF $current = NONE { THROW "computer_not_found"; };
LET $state = SELECT * FROM ONLY $computer;
LET $family = SELECT * FROM ONLY $current.family;
LET $limits = SELECT * FROM ONLY $policy;
IF $admission_expires_at <= $now OR $current = NONE OR $current.owner_key != $owner_key
  OR $current.ticket_hash != $ticket_hash OR $current.connection_id != NONE
  OR $current.revoked_at != NONE OR $current.ticket_expires_at <= $now
  OR $current.expires_at <= $now OR $current.idle_expires_at <= $now
  OR $family = NONE OR $family.revoked_at != NONE OR $family.expires_at <= $now
  OR $limits = NONE OR $limits.max_grants = 0 {
  THROW "computer_authority_expired";
};
IF $state = NONE OR $state.owner_key != $owner_key OR $state.phase != "ready" OR $state.active_operation != NONE
  OR $state.provider_instance_id != $provider OR $current.provider_instance_id != $provider
  OR $state.provider_resource_id != $current.provider_resource_id OR $state.process_id != $current.process_id {
  THROW "computer_invalid_state";
};
UPDATE $grant SET connection_id = $connection;
CREATE outbox_event CONTENT $event;
COMMIT TRANSACTION;
RETURN $connection;
