LET $revision_current = fn::kernel::gateway::control_revision_current_v1($authority_revision, $dispatch_authority.control_revision, $dispatch_authority.control_sha256);
LET $tenant = fn::kernel::identity::tenant_current_v1($authority_enterprise, $authority_tenant);
LET $source = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_source);
LET $actor = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_actor);
IF $authority_expires_at <= time::now()
  OR !$revision_current
  OR $tenant = NONE
  OR $source = NONE
  OR $actor = NONE {
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
  LET $owner_source = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_owner_source);
  LET $owner_actor = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_owner_actor);
  IF $grant_expires_at <= $now OR $owner_source = NONE OR $owner_actor = NONE
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
fn::append_audit($audit_rows);
