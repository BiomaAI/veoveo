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
IF $current = NONE OR $current.owner_key != $owner_key OR $current.provider_instance_id != $provider { THROW "computer_not_found"; };
LET $prior = SELECT * FROM ONLY $request;
IF $prior != NONE {
  IF $prior.fingerprint != $fingerprint { THROW "computer_request_conflict"; };
} ELSE {
  LET $grantee = SELECT * FROM ONLY $content.grantee;
  IF $grantee = NONE OR $grantee.enabled != true OR $grantee.tenant != $authority_tenant
    OR $grantee.issuer != $content.grantee_issuer OR $grantee.subject != $content.grantee_subject
    OR $grantee.kind != $content.grantee_kind { THROW "computer_authority_expired"; };
  LET $limits = (UPDATE ONLY $policy SET admissions += 1 WHERE fingerprint = $policy_fingerprint RETURN AFTER);
  IF $limits = NONE { THROW "computer_policy_conflict"; };
  IF $limits.max_grants = 0 { THROW "computer_authority_expired"; };
  IF $content.expires_at <= $now OR $content.expires_at > $now + $maximum_lifetime {
    THROW "computer_invalid_input";
  };
  IF $content.execution_limits != NONE AND
    ($content.execution_limits.maximumSeconds > $limits.maximum_execution_seconds
     OR $content.execution_limits.maximumOutputBytes > $limits.maximum_output_bytes) { THROW "computer_invalid_input"; };
  UPSERT $guard SET revision = (revision ?? 0) + 1;
  LET $active = SELECT VALUE grant_id FROM computer_automation_grant WHERE owner_key = $owner_key
    AND computer_id = $computer_id AND revoked_at = NONE AND expires_at > $now LIMIT 64;
  IF array::len($active) >= $limits.max_grants { THROW "computer_access_limit"; };
  CREATE $grant CONTENT object::extend($content, {issued_at:$now, revision:0});
  CREATE $request CONTENT {grant:$grant, fingerprint:$fingerprint};
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
SELECT * FROM ONLY (SELECT VALUE grant FROM ONLY $request);
