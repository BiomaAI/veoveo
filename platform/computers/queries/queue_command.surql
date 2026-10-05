BEGIN TRANSACTION;
LET $now = time::now();
LET $revision_current = fn::kernel::gateway::control_revision_current_v1($authority_revision, $authority_revision_id, $authority_sha256);
LET $tenant = fn::kernel::identity::tenant_current_v1($authority_enterprise, $authority_tenant);
LET $source = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_source);
LET $actor = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_actor);
LET $owner_source = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_owner_source);
LET $owner_actor = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_owner_actor);
IF $authority_expires_at <= $now OR $grant_expires_at <= $now
  OR !$revision_current OR $tenant = NONE OR $source = NONE OR $actor = NONE
  OR $owner_source = NONE OR $owner_actor = NONE { THROW "computer_authority_expired"; };
IF $family != NONE {
  LET $family_state = fn::kernel::gateway::refresh_family_current_v1($family, $now);
  IF $family_state = NONE OR $family_state.expires_at <= $now { THROW "computer_authority_expired"; };
};
LET $grant_state = (UPDATE ONLY $grant SET revision = revision
  WHERE revision = $grant_revision AND revoked_at = NONE AND expires_at > $now RETURN AFTER);
IF $grant_state = NONE OR $grant_state.computer_id != $content.computer_id
  OR $grant_state.grantee != $authority_source OR $grant_state.provider_instance_id != $content.provider_instance_id
  OR $grant_state.owner_key != $content.binding.owner_key OR "execute" NOT IN $grant_state.permissions
  { THROW "computer_authority_expired"; };
LET $limits = (UPDATE ONLY $policy SET admissions += 1 WHERE fingerprint = $policy_fingerprint RETURN AFTER);
IF $limits = NONE { THROW "computer_policy_conflict"; };
IF $limits.max_grants = 0 { THROW "computer_authority_expired"; };
LET $prior = SELECT * FROM ONLY $request;
IF $prior = NONE {
  LET $current = SELECT * FROM ONLY $computer;
  IF $current = NONE OR $current.owner_key != $content.binding.owner_key { THROW "computer_not_found"; };
  IF $current.active_operation != NONE OR (SELECT * FROM ONLY $slot) != NONE { THROW "computer_operation_busy"; };
  IF $current.phase != "ready" { THROW "computer_invalid_state"; };
  IF $current.updated_at != $expected_updated_at
    OR $current.owner_context != $expected_owner_context
    OR $current.provider_instance_id != $content.provider_instance_id
    OR $current.template_fingerprint != $content.binding.template_fingerprint
    OR <string>($current.replacement_instance_id ?? "") != ($content.binding.replacement_instance_id ?? "")
    OR $current.provider_resource_id != $content.binding.resource_id
    OR $current.process_id != $content.binding.process_id { THROW "computer_state_conflict"; };
  UPDATE $computer SET updated_at = updated_at;
  CREATE $slot CONTENT {execution:$execution, computer_id:$content.computer_id};
  CREATE $execution CONTENT object::extend($content, {stage:"queued", created_at:$now, updated_at:$now});
  CREATE $payload CONTENT $payload_content;
  CREATE $request CONTENT {execution:$execution};
  fn::append_audit($audit_rows);
};
COMMIT TRANSACTION;
