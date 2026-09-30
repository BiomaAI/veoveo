SELECT *
FROM ONLY $computer
WHERE
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
  AND (
    owner_context.authority.output_policy.data_labels ?? []
  ) ALLINSIDE $owner_labels
  AND (
    owner_context.authority.output_policy.classification = NONE
    OR owner_context.authority.output_policy.classification IN $owner_labels
  )
  AND provider_instance_id = $provider
  AND owner_key = $owner_key
  AND $admission_end > time::now();
