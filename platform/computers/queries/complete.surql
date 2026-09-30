SELECT *
FROM computer
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
  AND string::starts_with(<string> computer_id, $prefix)
ORDER BY computer_id ASC
LIMIT 101;
