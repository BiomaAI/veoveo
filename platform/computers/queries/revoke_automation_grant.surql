BEGIN TRANSACTION;
LET $now = time::now();
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
LET $current = SELECT * FROM ONLY $computer;
LET $grant_state = SELECT * FROM ONLY $grant;
IF $current = NONE OR $current.owner_key != $owner_key OR $current.provider_instance_id != $provider
  OR $grant_state = NONE OR $grant_state.owner_key != $owner_key OR $grant_state.computer_id != $computer_id
  OR $grant_state.provider_instance_id != $provider { THROW "computer_not_found"; };
IF $grant_state.revoked_at = NONE {
  UPDATE $grant SET revoked_at=$now, revision += 1;
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
SELECT * FROM ONLY $grant;
