-- Private authorization keys only; the public page is selected by admitted_computers.
LET $owned = SELECT VALUE computer_id
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
  AND provider_instance_id = $provider
  AND string::starts_with(<string> computer_id, $prefix)
  AND (
    $after = NONE
    OR computer_id > $after
  )
ORDER BY computer_id
LIMIT $limit;
LET $granted = SELECT VALUE computer_id
FROM computer_automation_grant
WHERE
  grantee = $grantee
  AND principal_id = $grantee_principal
  AND grantee_kind = $grantee_kind
  AND grantee_issuer = $grantee_issuer
  AND grantee_subject = $grantee_subject
  AND oauth_client_id = $client
  AND authority.invocation.tenant = $grantee_tenant
  AND authority.invocation.work_context = $context
  AND authority.profile = $profile
  AND provider_instance_id = $provider
  AND 'read' IN permissions
  AND string::starts_with(<string> computer_id, $prefix)
  AND revoked_at = NONE
  AND expires_at > time::now()
  AND (
    $after = NONE
    OR computer_id > $after
  )
GROUP BY computer_id
ORDER BY computer_id
LIMIT $limit;
RETURN array::slice(
  array::sort(array::distinct(array::concat($owned, $granted))),
  0,
  $limit
);
