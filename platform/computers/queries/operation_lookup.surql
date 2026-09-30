SELECT
  computer_id,
  action,
  automation_grant_id
FROM ONLY $operation
WHERE
  provider_instance_id = $provider
  AND (
    (
      $as_owner = true
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
      AND (
        owner_context.authority.output_policy.data_labels ?? []
      ) ALLINSIDE $owner_labels
      AND (
        owner_context.authority.output_policy.classification = NONE
        OR owner_context.authority.output_policy.classification IN $owner_labels
      )
    )
    OR (
      $as_owner = false
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
      AND (
        actor_context.authority.output_policy.data_labels ?? []
      ) ALLINSIDE $owner_labels
      AND (
        actor_context.authority.output_policy.classification = NONE
        OR actor_context.authority.output_policy.classification IN $owner_labels
      )
    )
  );
