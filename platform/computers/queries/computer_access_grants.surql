LET $parent = SELECT *
FROM ONLY $computer_record
WHERE
  tenant_key = $grantee_tenant
  AND provider_instance_id = $provider
  AND owner_context.data_labels ALLINSIDE $grantee_labels
  AND (
    owner_context.authority.output_policy.data_labels ?? []
  ) ALLINSIDE $grantee_labels
  AND (
    owner_context.authority.output_policy.classification = NONE
    OR owner_context.authority.output_policy.classification IN $grantee_labels
  );
SELECT VALUE grant_id
FROM computer_automation_grant
WHERE
  $parent != NONE
  AND computer_id = $computer
  AND provider_instance_id = $provider
  AND owner_key = $parent.owner_key
  AND authority.actor.id = $parent.owner_context.principal_key
  AND authority.actor.kind = $parent.owner_context.principal_kind
  AND authority.actor.issuer = $parent.owner_context.issuer
  AND authority.actor.subject = $parent.owner_context.subject
  AND grantee = $grantee
  AND principal_id = $grantee_principal
  AND grantee_kind = $grantee_kind
  AND grantee_issuer = $grantee_issuer
  AND grantee_subject = $grantee_subject
  AND oauth_client_id = $client
  AND authority.invocation.tenant = $grantee_tenant
  AND authority.invocation.work_context = $context
  AND authority.profile = $profile
  AND revoked_at = NONE
  AND expires_at > time::now()
  AND (
    $read_only = false
    OR 'read' IN permissions
  )
ORDER BY grant_id
LIMIT 65;
