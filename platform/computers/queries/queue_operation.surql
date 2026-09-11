BEGIN TRANSACTION;
IF time::now() >= $admission_expires_at { THROW "computer_authority_expired"; };
IF $automation {
  LET $now = time::now();
  LET $head = SELECT * FROM ONLY gateway_control_active:current;
  LET $revision = SELECT * FROM ONLY $authority_revision;
  LET $enterprise = SELECT * FROM ONLY $authority_enterprise;
  LET $tenant = SELECT * FROM ONLY $authority_tenant;
  LET $source = SELECT * FROM ONLY $authority_source;
  LET $actor = SELECT * FROM ONLY $authority_actor;
  LET $owner_source = SELECT * FROM ONLY $authority_owner_source;
  LET $owner_actor = SELECT * FROM ONLY $authority_owner_actor;
  IF $authority_expires_at <= $now OR $grant_expires_at <= $now
    OR $head = NONE OR $head.revision != $authority_revision OR $head.revision_id != $authority_revision_id
    OR $revision = NONE OR $revision.sha256 != $authority_sha256
    OR $enterprise.enabled != true OR $tenant.enabled != true OR $source.enabled != true OR $actor.enabled != true
    OR $owner_source.enabled != true OR $owner_actor.enabled != true { THROW "computer_authority_expired"; };
  IF $family != NONE {
    LET $family_state = SELECT * FROM ONLY $family;
    IF $family_state = NONE OR $family_state.revoked_at != NONE OR $family_state.expires_at <= $now { THROW "computer_authority_expired"; };
  };
  LET $grant_state = (UPDATE ONLY $grant SET revision = revision
    WHERE revision = $grant_revision AND revoked_at = NONE AND expires_at > $now RETURN AFTER);
  IF $grant_state = NONE OR $grant_state.grant_id != $content.automation_grant_id
    OR $grant_state.computer_id != $content.computer_id OR $grant_state.grantee != $authority_source
    OR $grant_state.provider_instance_id != $content.provider_instance_id OR $grant_state.owner_key != $owner_key
    OR $content.action NOT IN ["start", "stop"] OR $content.action NOT IN $grant_state.permissions
    { THROW "computer_authority_expired"; };
  LET $limits = (UPDATE ONLY $policy SET admissions += 1 WHERE fingerprint = $policy_fingerprint RETURN AFTER);
  IF $limits = NONE { THROW "computer_policy_conflict"; };
  IF $limits.max_grants = 0 { THROW "computer_authority_expired"; };
};
LET $prior = SELECT * FROM ONLY $request;
IF $prior != NONE {
  IF $prior.fingerprint != $fingerprint { THROW "computer_request_conflict"; };
} ELSE {
  LET $current = SELECT * FROM ONLY $computer;
  IF $current = NONE OR $current.owner_key != $owner_key { THROW "computer_not_found"; };
  IF $current.active_operation != NONE { THROW "computer_operation_busy"; };
  IF $previous_phase = "stopped" AND (SELECT * FROM ONLY $execution_slot) != NONE {
    THROW "computer_operation_busy";
  };
  IF $current.updated_at != $expected_updated_at
    OR $current.owner_context != $expected_owner_context
    OR $current.provider_instance_id != $content.provider_instance_id
    OR $current.replacement_instance_id != $content.replacement_instance_id
    OR $current.template_fingerprint != $content.template_fingerprint {
    THROW "computer_state_conflict";
  };
  IF $current.phase != $previous_phase { THROW "computer_invalid_state"; };
  IF $previous_phase != "reserved" AND
    ($current.provider_resource_id = NONE OR $current.provider_resource_id = ""
     OR $current.process_id = NONE OR $current.process_id = "") {
    THROW "computer_invalid_state";
  };
  LET $fenced = (UPDATE ONLY $computer SET active_operation = $content.operation_id,
    phase = $next_phase, updated_at = time::now()
    WHERE active_operation = NONE AND phase = $previous_phase
      AND owner_key = $owner_key AND updated_at = $expected_updated_at
      AND provider_instance_id = $content.provider_instance_id
      AND replacement_instance_id = $content.replacement_instance_id
      AND template_fingerprint = $content.template_fingerprint RETURN AFTER);
  IF $fenced = NONE { THROW "computer_operation_busy"; };
  CREATE $operation CONTENT object::extend($content, {
    previous_phase: $current.phase,
    previous_resource_id: $current.provider_resource_id,
    previous_process_id: $current.process_id
  });
  CREATE $request CONTENT {operation_id: $content.operation_id, fingerprint: $fingerprint};
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
