LET $now = time::now();
LET $current = SELECT * FROM ONLY $execution;
LET $head = SELECT * FROM ONLY gateway_control_active:current;
LET $revision = SELECT * FROM ONLY $authority_revision;
LET $enterprise = SELECT * FROM ONLY $authority_enterprise;
LET $tenant = SELECT * FROM ONLY $authority_tenant;
LET $source = SELECT * FROM ONLY $authority_source;
LET $actor = SELECT * FROM ONLY $authority_actor;
IF $authority_expires_at <= $now OR $head = NONE OR $head.revision != $authority_revision
  OR $head.revision_id != $authority_revision_id OR $revision = NONE OR $revision.sha256 != $authority_sha256
  OR $enterprise.enabled != true OR $tenant.enabled != true OR $source.enabled != true OR $actor.enabled != true {
  THROW "computer_authority_expired";
};
IF $family != NONE {
  LET $family_state = SELECT * FROM ONLY $family;
  IF $family_state = NONE OR $family_state.revoked_at != NONE OR $family_state.expires_at <= $now { THROW "computer_authority_expired"; };
};
IF $grant != NONE {
  LET $owner_source = SELECT * FROM ONLY $authority_owner_source;
  LET $owner_actor = SELECT * FROM ONLY $authority_owner_actor;
  IF $owner_source.enabled != true OR $owner_actor.enabled != true OR $grant_expires_at <= $now { THROW "computer_authority_expired"; };
  LET $grant_state = (UPDATE ONLY $grant SET revision = revision WHERE revision = $grant_revision AND revoked_at = NONE AND expires_at > $now RETURN AFTER);
  IF $grant_state = NONE OR $grant_state.computer_id != $current.computer_id
    OR $grant_state.grantee != $authority_source OR $grant_state.provider_instance_id != $provider
    OR $grant_state.owner_key != $current.binding.owner_key OR "execute" NOT IN $grant_state.permissions { THROW "computer_authority_expired"; };
  LET $limits = (UPDATE ONLY $policy SET admissions += 1 WHERE fingerprint = $policy_fingerprint RETURN AFTER);
  IF $limits = NONE { THROW "computer_policy_conflict"; };
  IF $limits.max_grants = 0 { THROW "computer_authority_expired"; };
};
LET $computer_state = SELECT * FROM ONLY $computer;
LET $slot_state = SELECT * FROM ONLY $slot;
IF $current = NONE OR $computer_state = NONE OR $slot_state = NONE
  OR $current.stage != "queued" OR $current.dispatch_id != NONE
  OR $current.created_at + $preparation_budget <= $now
  OR $current.transfer_id != $transfer_id OR $current.binding != $expected_binding
  OR $current.artifact_access = NONE OR $current.artifact_access != $expected_access
  OR $current.task != $_provider_task OR $slot_state.execution != $execution
  OR $slot_state.computer_id != $current.computer_id
  OR $current.computer_id != $computer_state.computer_id
  OR $current.provider_instance_id != $provider OR $computer_state.provider_instance_id != $provider
  OR $computer_state.owner_key != $current.binding.owner_key
  OR $computer_state.owner_context != $expected_owner_context
  OR $computer_state.template_fingerprint != $current.binding.template_fingerprint
  OR <string>($computer_state.replacement_instance_id ?? $computer_state.computer_id) != $current.binding.instance_id
  OR $computer_state.provider_resource_id != $current.binding.resource_id
  OR $computer_state.process_id != $current.binding.process_id
  OR $computer_state.phase != "ready" OR $computer_state.active_operation != NONE
  { THROW "computer_state_conflict"; };
LET $deadline = IF $execution_expiry != NONE AND $execution_expiry < $now + $duration { $execution_expiry } ELSE { $now + $duration };
IF $deadline <= $now { THROW "computer_authority_expired"; };
IF $access_expires <= $deadline + $publication_budget { THROW "computer_state_conflict"; };
-- This write conflicts with a concurrent lifecycle Stop admission.
UPDATE $computer SET updated_at = updated_at;
LET $dispatched = (UPDATE ONLY $execution SET stage = "dispatched", dispatch_id = $dispatch_id,
  dispatch_authority = $decision, effective_limits = $effective_limits,
  dispatched_at = $now, execution_deadline = $deadline, updated_at = $now
  WHERE stage = "queued" AND dispatch_id = NONE AND updated_at = $current.updated_at RETURN AFTER);
IF $dispatched = NONE { THROW "computer_state_conflict"; };
CREATE outbox_event CONTENT $event;
