LET $policy_head = SELECT * FROM ONLY gateway_control_active:current;
LET $policy_revision = SELECT * FROM ONLY $authority_revision;
LET $enterprise = SELECT * FROM ONLY $authority_enterprise;
LET $tenant = SELECT * FROM ONLY $authority_tenant;
LET $source = SELECT * FROM ONLY $authority_source;
LET $actor = SELECT * FROM ONLY $authority_actor;
IF $authority_expires_at <= time::now()
  OR $policy_head = NONE OR $policy_head.revision != $authority_revision
  OR $policy_head.revision_id != $dispatch_authority.control_revision
  OR $policy_revision = NONE OR $policy_revision.sha256 != $dispatch_authority.control_sha256
  OR $enterprise = NONE OR $enterprise.enabled != true
  OR $tenant = NONE OR $tenant.enabled != true
  OR $source = NONE OR $source.enabled != true
  OR $actor = NONE OR $actor.enabled != true {
  THROW "computer_current_authority_changed";
};
LET $current = SELECT * FROM ONLY $operation;
LET $computer_state = SELECT * FROM ONLY $computer;
IF $current = NONE OR $computer_state = NONE
  OR $current.stage != "queued" OR $current.dispatch_id != NONE
  OR $current.operation_id != $operation_id
  OR $current.computer_id != $computer_state.computer_id
  OR $current.owner_context != $expected_owner_context
  OR $computer_state.owner_context != $expected_owner_context
  OR $computer_state.owner_key != $owner_key
  OR $computer_state.active_operation != $operation_id
  OR $current.provider_instance_id != $provider
  OR $computer_state.provider_instance_id != $provider
  OR $current.template_fingerprint != $computer_state.template_fingerprint
  OR $current.replacement_instance_id != $computer_state.replacement_instance_id
  OR $current.previous_resource_id != $computer_state.provider_resource_id
  OR $current.previous_process_id != $computer_state.process_id {
  THROW "computer_state_conflict";
};
IF $automation {
  LET $now = time::now();
  LET $owner_source = SELECT * FROM ONLY $authority_owner_source;
  LET $owner_actor = SELECT * FROM ONLY $authority_owner_actor;
  IF $grant_expires_at <= $now OR $owner_source.enabled != true OR $owner_actor.enabled != true
    { THROW "computer_authority_expired"; };
  LET $grant_state = (UPDATE ONLY $grant SET revision = revision
    WHERE revision = $grant_revision AND revoked_at = NONE AND expires_at > $now RETURN AFTER);
  IF $grant_state = NONE OR $grant_state.grant_id != $current.automation_grant_id
    OR $grant_state.computer_id != $current.computer_id OR $grant_state.grantee != $authority_source
    OR $grant_state.provider_instance_id != $provider OR $grant_state.owner_key != $owner_key
    OR $current.action NOT IN ["start", "stop"] OR $current.action NOT IN $grant_state.permissions
    OR $dispatch_authority.automation.grant_id != <string>$current.automation_grant_id
    OR $dispatch_authority.automation.grant_revision != $grant_revision
    { THROW "computer_authority_expired"; };
  LET $limits = (UPDATE ONLY $policy SET admissions += 1 WHERE fingerprint = $policy_fingerprint RETURN AFTER);
  IF $limits = NONE { THROW "computer_policy_conflict"; };
  IF $limits.max_grants = 0 { THROW "computer_authority_expired"; };
} ELSE IF $current.automation_grant_id != NONE {
  THROW "computer_authority_expired";
};
LET $dispatched = (UPDATE ONLY $operation SET stage = "dispatched", dispatch_id = $dispatch_id,
  dispatch_authority = $dispatch_authority,
  dispatched_at = time::now(), observation_deadline = time::now() + 180s,
  updated_at = time::now() WHERE stage = "queued" AND dispatch_id = NONE
    AND updated_at = $current.updated_at RETURN AFTER);
IF $dispatched = NONE { THROW "computer_state_conflict"; };
CREATE outbox_event CONTENT $event;
