RETURN {
  LET $candidate = SELECT computer_id, actor_context FROM ONLY $maintenance
  WHERE provider_instance_id = $provider
    AND ($expected_computer = NONE OR computer_id = $expected_computer)
    AND actor_context.authority.tenant = $owner_tenant
    AND (
      actor_context.tenant_key ?? 'installation'
    ) = $owner_tenant
    AND actor_context.principal_key = $owner_principal
    AND actor_context.principal_kind = $owner_kind
    AND actor_context.issuer = $owner_issuer
    AND actor_context.subject = $owner_subject
    AND actor_context.authority.work_context = $owner_context
    AND actor_context.data_labels ALLINSIDE $owner_labels
    AND type::is_object(actor_context.authority)
  AND type::is_object(actor_context.authority.output_policy)
  AND (
    actor_context.authority.output_policy.data_labels
  ) ALLINSIDE $owner_labels
    AND (
      actor_context.authority.output_policy.classification = NONE
      OR actor_context.authority.output_policy.classification = NULL
      OR actor_context.authority.output_policy.classification IN $owner_labels
    );
  LET $owned = SELECT VALUE computer_id FROM computer
  WHERE computer_id = $candidate.computer_id AND provider_instance_id = $provider AND
    tenant_key = $owner_tenant
    AND owner_context.authority.tenant = $owner_tenant
    AND (
      owner_context.tenant_key ?? 'installation'
    ) = $owner_tenant
    AND owner_context.principal_key = $owner_principal
    AND owner_context.principal_kind = $owner_kind
    AND owner_context.issuer = $owner_issuer
    AND owner_context.subject = $owner_subject
    AND owner_context.authority.work_context = $owner_context
    AND owner_context.data_labels ALLINSIDE $owner_labels
    AND type::is_object(owner_context.authority)
  AND type::is_object(owner_context.authority.output_policy)
  AND (
    owner_context.authority.output_policy.data_labels
  ) ALLINSIDE $owner_labels
    AND (
      owner_context.authority.output_policy.classification = NONE
      OR owner_context.authority.output_policy.classification = NULL
      OR owner_context.authority.output_policy.classification IN $owner_labels
    );
  SELECT * FROM ONLY $maintenance
  WHERE provider_instance_id = $provider AND computer_id IN $owned
    AND actor_context = $candidate.actor_context;
};
