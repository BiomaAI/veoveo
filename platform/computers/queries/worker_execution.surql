RETURN {
  LET $exists = SELECT VALUE id FROM ONLY $journal;
  LET $admitted = SELECT *, payload.sealed AS sealed FROM ONLY $journal
  WHERE id != NONE AND provider_instance_id = $provider
    AND computer_id = $computer_id AND task = $task
    AND authority.actor.id = $actor.principal_key
    AND authority.actor.kind = $actor.principal_kind
    AND authority.actor.issuer = $actor.issuer
    AND authority.actor.subject = $actor.subject
    AND (authority.actor.tenant ?? NONE) = ($actor.tenant_key ?? NONE)
    AND authority.actor.data_labels = $actor.data_labels
    AND authority.profile = $actor.profile AND authority.invocation = $actor.authority;
  IF $exists != NONE AND $admitted = NONE { THROW "computer_state_conflict"; };
  $admitted;
};
RETURN time::now();
