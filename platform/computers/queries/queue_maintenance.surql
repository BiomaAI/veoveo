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
LET $prior = SELECT * FROM ONLY $request;
IF $prior != NONE {
  IF $prior.fingerprint != $fingerprint { THROW "computer_request_conflict"; };
} ELSE {
  LET $current = SELECT * FROM ONLY $computer;
  IF $current = NONE OR $current.owner_key != $content.owner_key { THROW "computer_not_found"; };
  IF (SELECT * FROM ONLY $execution_slot) != NONE { THROW "computer_operation_busy"; };
  IF $current.active_operation != $expected_active_operation { THROW "computer_operation_busy"; };
  IF $current.updated_at != $expected_updated_at OR $current.owner_context != $expected_owner_context
    OR $current.provider_instance_id != $content.provider_instance_id
    OR $current.template_id != $content.source_template_id
    OR $current.template_fingerprint != $content.source_template_fingerprint
    OR $current.replacement_instance_id != $expected_replacement
    OR ($current.replacement_instance_id ?? $current.computer_id) != $content.source_instance_id
    OR $current.phase != $expected_phase OR $current.provider_resource_id != $expected_resource_id
    OR $current.process_id != $expected_process_id { THROW "computer_state_conflict"; };
  IF $source_operation != NONE {
    LET $original = SELECT * FROM ONLY $source_operation;
    IF $current.phase != "recovery_required" OR $current.replacement_instance_id != NONE
      OR $current.provider_resource_id != NONE OR $current.process_id != NONE
      OR $original = NONE OR $original.operation_id != $current.active_operation
      OR $original.operation_id != <uuid>$content.source.operation_id
      OR $original.computer_id != $current.computer_id OR $original.action != "create"
      OR $original.stage != "recovery_required" OR $original.replacement_instance_id != NONE
      OR $original.provider_instance_id != $current.provider_instance_id
      OR $original.template_fingerprint != $current.template_fingerprint
      OR $original.dispatch_id = NONE OR $original.observation_deadline = NONE
      OR $original.observation_deadline > $now { THROW "computer_invalid_state"; };
  } ELSE {
    IF $current.active_operation != NONE OR $current.phase NOT IN ["ready", "stopped"] { THROW "computer_operation_busy"; };
  };
  LET $fenced = (UPDATE ONLY $computer SET active_operation = $content.operation_id, updated_at = $now
    WHERE active_operation = $expected_active_operation AND updated_at = $expected_updated_at RETURN AFTER);
  IF $fenced = NONE { THROW "computer_state_conflict"; };
  CREATE $maintenance CONTENT $content;
  CREATE $request CONTENT {operation_id: $content.operation_id, fingerprint: $fingerprint};
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
