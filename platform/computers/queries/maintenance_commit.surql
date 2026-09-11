LET $now = time::now();
LET $current = SELECT * FROM ONLY $maintenance;
LET $home = SELECT * FROM ONLY $computer;
IF $current = NONE OR $home = NONE OR $current.operation_id != $operation_id
  OR $current.provider_instance_id != $provider OR $home.provider_instance_id != $provider
  OR $current.computer_id != $home.computer_id OR $home.active_operation != $operation_id
  OR $current.updated_at != $expected_updated_at OR $current.progress != $expected_progress OR $current.stage != $expected_stage
  OR $home.updated_at != $computer_updated_at OR $home.owner_context != $source_owner
  OR $home.template_fingerprint != $current.source_template_fingerprint OR $home.template_id != $current.source_template_id
  OR ($home.replacement_instance_id ?? $home.computer_id) != $current.source_instance_id
  OR ($current.source.kind IN ["ready", "stopped"] AND ($home.provider_resource_id != $current.source.resource_id OR $home.process_id != $current.source.process_id))
  OR ($current.source.kind = "initial_failure" AND ($home.provider_resource_id != NONE OR $home.process_id != NONE)) {
  THROW "computer_state_conflict";
};
IF $policy != NONE {
  LET $head = SELECT * FROM ONLY gateway_control_active:current;
  LET $revision = SELECT * FROM ONLY $authority_revision;
  LET $enterprise = SELECT * FROM ONLY $authority_enterprise;
  LET $tenant = SELECT * FROM ONLY $authority_tenant;
  LET $source = SELECT * FROM ONLY $authority_source;
  LET $actor = SELECT * FROM ONLY $authority_actor;
  IF <datetime>$policy.valid_until <= $now
    OR $head = NONE OR $head.revision != $authority_revision OR $head.revision_id != $policy.control_revision
    OR $revision = NONE OR $revision.sha256 != $policy.control_sha256
    OR $enterprise.enabled != true OR $tenant.enabled != true OR $source.enabled != true OR $actor.enabled != true {
    THROW "computer_authority_expired";
  };
};
IF $checkpoint != NONE {
  IF $expected_stage != "capturing" OR (SELECT * FROM ONLY $policy_record) != NONE { THROW "computer_state_conflict"; };
  CREATE $policy_record CONTENT {operation_id: $operation_id, envelope: $checkpoint};
};
LET $changed = (UPDATE ONLY $maintenance SET progress = $progress, stage = $stage, updated_at = $now
  WHERE updated_at = $expected_updated_at AND stage = $expected_stage RETURN AFTER);
IF $changed = NONE { THROW "computer_state_conflict"; };
IF $stage = "succeeded" {
  IF $expected_stage != "adopting" OR $policy = NONE OR $target_resource = NONE OR $target_process = NONE {
    THROW "computer_state_conflict";
  };
  UPDATE ONLY $computer SET replacement_instance_id = $current.target_instance_id,
    template_id = $current.target_template_id, template_fingerprint = $current.target_template_fingerprint,
    provider_resource_id = $target_resource, process_id = $target_process, phase = "ready", active_operation = NONE, updated_at = $now;
} ELSE IF $stage = "cancelled" {
  IF $expected_stage != "queued" OR array::len($expected_progress.steps) != 0 { THROW "computer_state_conflict"; };
  UPDATE ONLY $computer SET active_operation = $initial_operation, updated_at = $now;
} ELSE IF $stage = "recovery_required" {
  UPDATE ONLY $computer SET phase = "recovery_required", updated_at = $now;
} ELSE IF $stage = "stopping" AND array::last($progress.steps).evidence.kind = "stopped" {
  UPDATE ONLY $computer SET phase = "stopped", updated_at = $now;
};
CREATE outbox_event CONTENT $event;
