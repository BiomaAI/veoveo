BEGIN TRANSACTION;
LET $now = time::now();
LET $head = SELECT * FROM ONLY gateway_control_active:current;
LET $revision = SELECT * FROM ONLY $authority_revision;
LET $enterprise = SELECT * FROM ONLY $authority_enterprise;
LET $tenant = SELECT * FROM ONLY $authority_tenant;
LET $source = SELECT * FROM ONLY $authority_source;
LET $actor = SELECT * FROM ONLY $authority_actor;
LET $family_state = SELECT * FROM ONLY $family;
IF $authority_expires_at <= $now OR $head = NONE OR $head.revision != $authority_revision
  OR $head.revision_id != $authority_revision_id OR $revision = NONE OR $revision.sha256 != $authority_sha256
  OR $enterprise.enabled != true OR $tenant.enabled != true OR $source.enabled != true OR $actor.enabled != true
  OR $family_state = NONE OR $family_state.revoked_at != NONE OR $family_state.expires_at <= $now {
  THROW "computer_authority_expired";
};
LET $current = SELECT * FROM ONLY $computer;
IF $current = NONE OR $current.owner_key != $owner_key { THROW "computer_not_found"; };
IF $current.phase != "ready" OR $current.active_operation != NONE OR $current.provider_instance_id != $provider
  OR $current.provider_resource_id != $resource OR $current.process_id != $process {
  THROW "computer_invalid_state";
};
LET $limits = (UPDATE ONLY $policy SET admissions += 1 WHERE fingerprint = $policy_fingerprint RETURN AFTER);
IF $limits = NONE { THROW "computer_policy_conflict"; };
UPSERT $guard SET revision = (revision ?? 0) + 1;
LET $active = SELECT VALUE grant_id FROM computer_session_grant WHERE computer_id = $computer_id AND owner_key = $owner_key
  AND revoked_at = NONE AND expires_at > $now AND idle_expires_at > $now
  AND family.revoked_at = NONE AND family.expires_at > $now
  AND (connection_id != NONE OR ticket_expires_at > $now) LIMIT 128;
IF $limits.max_grants = 0 { THROW "computer_authority_expired"; };
IF array::len($active) >= $limits.max_grants { THROW "computer_access_limit"; };
LET $absolute_expiry = $now + $absolute;
LET $expires = IF $absolute_expiry < $family_state.expires_at { $absolute_expiry } ELSE { $family_state.expires_at };
LET $idle_expiry = $now + $idle;
LET $idle_end = IF $idle_expiry < $expires { $idle_expiry } ELSE { $expires };
LET $ticket_end = IF $now + 30s < $idle_end { $now + 30s } ELSE { $idle_end };
CREATE $grant CONTENT {grant_id:$grant_id, computer_id:$computer_id, owner_key:$owner_key,
  provider_instance_id:$provider, authority:$authority, family:$family, ticket_hash:$ticket_hash,
  ticket_expires_at:$ticket_end, issued_at:$now, expires_at:$expires, idle_expires_at:$idle_end,
  last_activity_at:$now, provider_resource_id:$resource, process_id:$process};
CREATE outbox_event CONTENT $event;
COMMIT TRANSACTION;
SELECT VALUE ticket_expires_at FROM ONLY $grant;
