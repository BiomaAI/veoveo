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
  OR $computer_state.active_operation != $operation_id
  OR $current.provider_instance_id != $provider
  OR $computer_state.provider_instance_id != $provider
  OR $current.template_fingerprint != $computer_state.template_fingerprint
  OR $current.previous_resource_id != $computer_state.provider_resource_id
  OR $current.previous_process_id != $computer_state.process_id {
  THROW "computer_state_conflict";
};
LET $dispatched = (UPDATE ONLY $operation SET stage = "dispatched", dispatch_id = $dispatch_id,
  dispatch_authority = $dispatch_authority,
  dispatched_at = time::now(), observation_deadline = time::now() + 180s,
  updated_at = time::now() WHERE stage = "queued" AND dispatch_id = NONE
    AND updated_at = $current.updated_at RETURN AFTER);
IF $dispatched = NONE { THROW "computer_state_conflict"; };
CREATE outbox_event CONTENT $event;
