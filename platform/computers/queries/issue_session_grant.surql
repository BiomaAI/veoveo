BEGIN TRANSACTION;
LET $now = time::now();
LET $revision_current = fn::kernel::gateway::control_revision_current_v1($authority_revision, $authority_revision_id, $authority_sha256);
LET $tenant = fn::kernel::identity::tenant_current_v1($authority_enterprise, $authority_tenant);
LET $source = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_source);
LET $actor = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_actor);
LET $family_state = fn::kernel::gateway::refresh_family_current_v1($family, $now);
IF $authority_expires_at <= $now OR !$revision_current OR $tenant = NONE OR $source = NONE OR $actor = NONE
  OR $family_state = NONE OR $family_state.expires_at <= $now {
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
  AND fn::kernel::gateway::refresh_family_current_v1(family, $now) != NONE
  AND (connection_id != NONE OR ticket_expires_at > $now) LIMIT 128;
IF $limits.max_grants = 0 { THROW "computer_authority_expired"; };
LET $cli = SELECT VALUE grant_id FROM computer_cli_grant WHERE computer_id = $computer_id AND owner_key = $owner_key
  AND revoked_at = NONE AND expires_at > $now AND idle_expires_at > $now
  AND fn::kernel::gateway::refresh_family_current_v1(family, $now) != NONE LIMIT 128;
IF array::len($active) + array::len($cli) >= $limits.max_grants { THROW "computer_access_limit"; };
LET $absolute_expiry = $now + $absolute;
LET $expires = IF $absolute_expiry < $family_state.expires_at { $absolute_expiry } ELSE { $family_state.expires_at };
LET $idle_expiry = $now + $idle;
LET $idle_end = IF $idle_expiry < $expires { $idle_expiry } ELSE { $expires };
LET $ticket_end = IF $now + 30s < $idle_end { $now + 30s } ELSE { $idle_end };
CREATE $grant CONTENT {grant_id:$grant_id, computer_id:$computer_id, owner_key:$owner_key,
  provider_instance_id:$provider, authority:$authority, family:$family, ticket_hash:$ticket_hash,
  ticket_expires_at:$ticket_end, issued_at:$now, expires_at:$expires, idle_expires_at:$idle_end,
  last_activity_at:$now, provider_resource_id:$resource, process_id:$process};
fn::append_audit($audit_rows);
COMMIT TRANSACTION;
SELECT VALUE ticket_expires_at FROM ONLY $grant;
