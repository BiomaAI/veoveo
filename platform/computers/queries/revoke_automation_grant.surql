BEGIN TRANSACTION;
LET $now = time::now();
LET $revision_current = fn::kernel::gateway::control_revision_current_v1($authority_revision, $authority_revision_id, $authority_sha256);
LET $tenant = fn::kernel::identity::tenant_current_v1($authority_enterprise, $authority_tenant);
LET $source = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_source);
LET $actor = fn::kernel::identity::principal_current_v1($authority_tenant, $authority_actor);
IF $authority_expires_at <= $now OR !$revision_current OR $tenant = NONE OR $source = NONE OR $actor = NONE {
  THROW "computer_authority_expired";
};
IF $family != NONE {
  LET $family_state = fn::kernel::gateway::refresh_family_current_v1($family, $now);
  IF $family_state = NONE OR $family_state.expires_at <= $now { THROW "computer_authority_expired"; };
};
LET $current = SELECT * FROM ONLY $computer;
LET $grant_state = SELECT * FROM ONLY $grant;
IF $current = NONE OR $current.owner_key != $owner_key OR $current.provider_instance_id != $provider
  OR $grant_state = NONE OR $grant_state.owner_key != $owner_key OR $grant_state.computer_id != $computer_id
  OR $grant_state.provider_instance_id != $provider { THROW "computer_not_found"; };
IF $grant_state.revoked_at = NONE {
  UPDATE $grant SET revoked_at=$now, revision += 1;
  fn::append_audit($audit_rows);
};
COMMIT TRANSACTION;
SELECT * FROM ONLY $grant;
