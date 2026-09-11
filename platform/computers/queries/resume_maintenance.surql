-- The shared runtime owns the transaction, exact lease and cancellation guard.
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
IF (SELECT * FROM ONLY $request) != NONE { THROW "computer_request_conflict"; };
LET $current = SELECT * FROM ONLY $maintenance;
LET $home = SELECT * FROM ONLY $computer;
IF $current = NONE OR $home = NONE OR $current.operation_id != $operation_id
  OR $current.provider_instance_id != $provider OR $home.provider_instance_id != $provider
  OR $current.computer_id != $home.computer_id OR $home.active_operation != $operation_id
  OR $current.updated_at != $expected_updated_at OR $current.progress != $expected_progress
  OR $current.stage != "recovery_required" OR $current.task_projected_at != NONE
  OR $home.updated_at != $computer_updated_at OR $home.owner_context != $source_owner
  OR $home.template_fingerprint != $current.source_template_fingerprint OR $home.template_id != $current.source_template_id
  OR ($home.replacement_instance_id ?? $home.computer_id) != $current.source_instance_id
  OR ($current.source.kind IN ["ready", "stopped"] AND ($home.provider_resource_id != $current.source.resource_id OR $home.process_id != $current.source.process_id))
  OR ($current.source.kind = "initial_failure" AND ($home.provider_resource_id != NONE OR $home.process_id != NONE)) {
  THROW "computer_state_conflict";
};
CREATE $request CONTENT $content;
UPDATE ONLY $maintenance SET progress = $progress, stage = $stage, updated_at = $now;
-- The owner, physical instance, retained quota and exclusive fence remain intact.
UPDATE ONLY $computer SET updated_at = $now;
CREATE outbox_event CONTENT $event;
